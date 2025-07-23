#![forbid(unsafe_code)]

//! age-encrypted keystore utilities.
//!
//! This module provides helper functions to **persist** long-term secret keys
//! (node identity keys, etc.) to disk using the [age] file encryption format
//! recommended in the Nyx design document (§4.2 *Key Store*).  The keystore is
//! encrypted with a *passphrase* supplied by the caller at runtime and the
//! plaintext is **zeroised** from memory immediately after use.
//!
//! The API intentionally keeps I/O concerns minimal to remain flexible for
//! daemon and CLI use:
//!
//! * [`encrypt_and_store()`] – writes an age-encrypted file containing the     
//!   secret to the given path.
//! * [`load_and_decrypt()`] – decrypts the file and returns the secret wrapped
//!   in [`Zeroizing`] to guarantee memory cleansing on drop.
//!
//! ### Example
//! ```rust,no_run
//! use nyx_crypto::keystore::{encrypt_and_store, load_and_decrypt};
//! use zeroize::Zeroizing;
//!
//! let secret = Zeroizing::new(b"my secret key".to_vec());
//! encrypt_and_store(&secret, "./keys.json.age", "correct horse battery staple").unwrap();
//! let recovered = load_and_decrypt("./keys.json.age", "correct horse battery staple").unwrap();
//! assert_eq!(&*secret, &*recovered);
//! ```
//!
//! The implementation exclusively uses in-memory buffers; no plaintext is ever
//! written to disk.

use std::fs;
use std::io::{Read, Write, BufRead, BufReader};
use std::path::Path;
use age::armor::{ArmoredWriter, ArmoredReader, Format};
use age::secrecy::SecretString;
use age::Encryptor;
use zeroize::Zeroizing;
use thiserror::Error;
use std::time::{SystemTime, Duration};

/// Error type for keystore operations.
#[derive(Debug, Error)]
pub enum KeystoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("age encryption error: {0}")]
    Encrypt(#[from] age::EncryptError),
    #[error("age decryption error: {0}")]
    Decrypt(#[from] age::DecryptError),
    #[error("decryption failed (incorrect passphrase or corrupt file)")]
    DecryptFailed,
    #[error("maximum retry attempts exceeded")]
    MaxRetriesExceeded,
    #[error("user cancelled operation")]
    UserCancelled,
    #[error("no input available (non-interactive mode)")]
    NoInputAvailable,
}

/// Configuration for passphrase retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Whether to show password strength hints
    pub show_strength_hints: bool,
    /// Whether to confirm password on creation
    pub confirm_on_create: bool,
    /// Minimum password length
    pub min_length: usize,
    /// Whether to allow empty passwords (not recommended)
    pub allow_empty: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            show_strength_hints: true,
            confirm_on_create: true,
            min_length: 8,
            allow_empty: false,
        }
    }
}

/// Passphrase input mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    /// Interactive terminal input with hidden typing
    Interactive,
    /// Non-interactive mode (read from stdin)
    NonInteractive,
    /// Programmatic mode (passphrase provided directly)
    Programmatic(String),
}

/// Passphrase strength assessment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordStrength {
    VeryWeak,
    Weak,
    Fair,
    Good,
    Strong,
}

impl PasswordStrength {
    /// Assess password strength based on various criteria
    pub fn assess(password: &str) -> Self {
        let mut score = 0;
        
        // Length scoring
        if password.len() >= 8 { score += 1; }
        if password.len() >= 12 { score += 1; }
        if password.len() >= 16 { score += 1; }
        
        // Character variety scoring
        if password.chars().any(|c| c.is_ascii_lowercase()) { score += 1; }
        if password.chars().any(|c| c.is_ascii_uppercase()) { score += 1; }
        if password.chars().any(|c| c.is_ascii_digit()) { score += 1; }
        if password.chars().any(|c| !c.is_ascii_alphanumeric()) { score += 1; }
        
        // Avoid common patterns
        if !password.to_lowercase().contains("password") &&
           !password.to_lowercase().contains("123456") &&
           !password.chars().collect::<std::collections::HashSet<_>>().len() < 3 {
            score += 1;
        }
        
        match score {
            0..=2 => Self::VeryWeak,
            3..=4 => Self::Weak,
            5..=6 => Self::Fair,
            7..=8 => Self::Good,
            _ => Self::Strong,
        }
    }
    
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::VeryWeak => "Very Weak",
            Self::Weak => "Weak",
            Self::Fair => "Fair",
            Self::Good => "Good",
            Self::Strong => "Strong",
        }
    }
    
    /// Get color code for terminal display
    pub fn color_code(&self) -> &'static str {
        match self {
            Self::VeryWeak => "\x1b[31m", // Red
            Self::Weak => "\x1b[33m",     // Yellow
            Self::Fair => "\x1b[33m",     // Yellow
            Self::Good => "\x1b[32m",     // Green
            Self::Strong => "\x1b[92m",   // Bright Green
        }
    }
}

/// Secure passphrase input with retry logic
pub struct PassphraseInput {
    config: RetryConfig,
    mode: InputMode,
}

impl PassphraseInput {
    /// Create new passphrase input with default configuration
    pub fn new() -> Self {
        Self {
            config: RetryConfig::default(),
            mode: InputMode::Interactive,
        }
    }
    
    /// Create with custom configuration
    pub fn with_config(config: RetryConfig) -> Self {
        Self {
            config,
            mode: InputMode::Interactive,
        }
    }
    
    /// Set input mode
    pub fn with_mode(mut self, mode: InputMode) -> Self {
        self.mode = mode;
        self
    }
    
    /// Prompt for passphrase with retry logic for decryption
    pub fn prompt_for_decrypt(&self, prompt: &str) -> Result<Zeroizing<String>, KeystoreError> {
        match &self.mode {
            InputMode::Programmatic(pass) => Ok(Zeroizing::new(pass.clone())),
            InputMode::NonInteractive => self.read_from_stdin(),
            InputMode::Interactive => self.interactive_decrypt_prompt(prompt),
        }
    }
    
    /// Prompt for passphrase with confirmation for encryption
    pub fn prompt_for_encrypt(&self, prompt: &str) -> Result<Zeroizing<String>, KeystoreError> {
        match &self.mode {
            InputMode::Programmatic(pass) => {
                self.validate_password(pass)?;
                Ok(Zeroizing::new(pass.clone()))
            },
            InputMode::NonInteractive => {
                let pass = self.read_from_stdin()?;
                self.validate_password(&pass)?;
                Ok(pass)
            },
            InputMode::Interactive => self.interactive_encrypt_prompt(prompt),
        }
    }
    
    /// Interactive prompt for decryption with retries
    fn interactive_decrypt_prompt(&self, prompt: &str) -> Result<Zeroizing<String>, KeystoreError> {
        use std::io::{self, Write};
        
        for attempt in 1..=self.config.max_attempts {
            print!("{} (attempt {}/{}): ", prompt, attempt, self.config.max_attempts);
            io::stdout().flush().map_err(KeystoreError::Io)?;
            
            let password = self.read_password_hidden()?;
            
            if password.is_empty() && !self.config.allow_empty {
                eprintln!("Password cannot be empty. Please try again.");
                continue;
            }
            
            return Ok(password);
        }
        
        Err(KeystoreError::MaxRetriesExceeded)
    }
    
    /// Interactive prompt for encryption with confirmation
    fn interactive_encrypt_prompt(&self, prompt: &str) -> Result<Zeroizing<String>, KeystoreError> {
        use std::io::{self, Write};
        
        for attempt in 1..=self.config.max_attempts {
            print!("{}: ", prompt);
            io::stdout().flush().map_err(KeystoreError::Io)?;
            
            let password = self.read_password_hidden()?;
            
            // Validate password
            if let Err(e) = self.validate_password(&password) {
                eprintln!("Password validation failed: {}", e);
                continue;
            }
            
            // Show strength assessment
            if self.config.show_strength_hints {
                let strength = PasswordStrength::assess(&password);
                eprintln!("{}Password strength: {}{}\x1b[0m", 
                         strength.color_code(), 
                         strength.description(),
                         if matches!(strength, PasswordStrength::VeryWeak | PasswordStrength::Weak) {
                             " (consider using a stronger password)"
                         } else {
                             ""
                         });
            }
            
            // Confirm password if required
            if self.config.confirm_on_create {
                print!("Confirm password: ");
                io::stdout().flush().map_err(KeystoreError::Io)?;
                
                let confirm = self.read_password_hidden()?;
                
                if password.as_bytes() != confirm.as_bytes() {
                    eprintln!("Passwords do not match. Please try again.");
                    continue;
                }
            }
            
            return Ok(password);
        }
        
        Err(KeystoreError::MaxRetriesExceeded)
    }
    
    /// Read password with hidden input (no echo)
    fn read_password_hidden(&self) -> Result<Zeroizing<String>, KeystoreError> {
        #[cfg(unix)]
        {
            self.read_password_unix()
        }
        #[cfg(windows)]
        {
            self.read_password_windows()
        }
        #[cfg(not(any(unix, windows)))]
        {
            // Fallback for other platforms
            self.read_from_stdin()
        }
    }
    
    #[cfg(unix)]
    fn read_password_unix(&self) -> Result<Zeroizing<String>, KeystoreError> {
        use std::os::unix::io::AsRawFd;
        use std::io::{self, BufRead};
        
        let stdin = io::stdin();
        let fd = stdin.as_raw_fd();
        
        // Get current terminal attributes
        let mut termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut termios) } != 0 {
            return Err(KeystoreError::Io(io::Error::last_os_error()));
        }
        
        // Disable echo
        let mut new_termios = termios;
        new_termios.c_lflag &= !libc::ECHO;
        
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &new_termios) } != 0 {
            return Err(KeystoreError::Io(io::Error::last_os_error()));
        }
        
        // Read password
        let mut line = String::new();
        let result = stdin.lock().read_line(&mut line);
        
        // Restore terminal attributes
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) };
        
        // Print newline since echo was disabled
        println!();
        
        match result {
            Ok(_) => {
                // Remove trailing newline
                if line.ends_with('\n') {
                    line.pop();
                }
                if line.ends_with('\r') {
                    line.pop();
                }
                Ok(Zeroizing::new(line))
            },
            Err(e) => Err(KeystoreError::Io(e)),
        }
    }
    
    #[cfg(windows)]
    fn read_password_windows(&self) -> Result<Zeroizing<String>, KeystoreError> {
        use std::io::{self, Write};
        
        let mut password = String::new();
        
        loop {
            // Read single character
            let mut buffer = [0u8; 1];
            match io::stdin().read_exact(&mut buffer) {
                Ok(()) => {
                    let ch = buffer[0] as char;
                    
                    if ch == '\r' || ch == '\n' {
                        break;
                    } else if ch == '\x08' || ch == '\x7f' { // Backspace or DEL
                        if !password.is_empty() {
                            password.pop();
                            print!("\x08 \x08"); // Erase character
                            io::stdout().flush().map_err(KeystoreError::Io)?;
                        }
                    } else if ch.is_control() {
                        // Ignore other control characters
                        continue;
                    } else {
                        password.push(ch);
                        print!("*"); // Show asterisk for each character
                        io::stdout().flush().map_err(KeystoreError::Io)?;
                    }
                },
                Err(e) => return Err(KeystoreError::Io(e)),
            }
        }
        
        println!(); // Newline after password input
        Ok(Zeroizing::new(password))
    }
    
    /// Read from stdin (non-interactive mode)
    fn read_from_stdin(&self) -> Result<Zeroizing<String>, KeystoreError> {
        use std::io::{self, BufRead};
        
        let stdin = io::stdin();
        let mut line = String::new();
        
        match stdin.lock().read_line(&mut line) {
            Ok(0) => Err(KeystoreError::NoInputAvailable),
            Ok(_) => {
                // Remove trailing newline
                if line.ends_with('\n') {
                    line.pop();
                }
                if line.ends_with('\r') {
                    line.pop();
                }
                Ok(Zeroizing::new(line))
            },
            Err(e) => Err(KeystoreError::Io(e)),
        }
    }
    
    /// Validate password according to configuration
    fn validate_password(&self, password: &str) -> Result<(), KeystoreError> {
        if password.is_empty() && !self.config.allow_empty {
            return Err(KeystoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Password cannot be empty"
            )));
        }
        
        if password.len() < self.config.min_length {
            return Err(KeystoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Password must be at least {} characters long", self.config.min_length)
            )));
        }
        
        Ok(())
    }
}

impl Default for PassphraseInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Encrypt `secret` with `passphrase` and write to `path` in armored age format.
pub fn encrypt_and_store<P: AsRef<Path>>(secret: &Zeroizing<Vec<u8>>, path: P, passphrase: &str) -> Result<(), KeystoreError> {
    // Ensure parent directory exists.
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }

    // Writer → armor → file.
    let file = fs::File::create(&path)?;
    let armor = ArmoredWriter::wrap_output(file, Format::AsciiArmor)
        .map_err(|e| KeystoreError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    let pass = SecretString::from(passphrase.to_owned());
    let encryptor = Encryptor::with_user_passphrase(pass);
    let mut writer = encryptor.wrap_output(armor)?;

    writer.write_all(&*secret)?;
    writer.finish()?; // flush + close
    Ok(())
}

/// Load age-encrypted keystore from `path`, decrypt with `passphrase` and return secret.
pub fn load_and_decrypt<P: AsRef<Path>>(path: P, passphrase: &str) -> Result<Zeroizing<Vec<u8>>, KeystoreError> {
    let file = fs::File::open(&path)?;
    let mut armor = ArmoredReader::new(file);
    let decryptor = age::Decryptor::new(&mut armor)?;

    let pass = SecretString::from(passphrase.to_owned());
    let identity = age::scrypt::Identity::new(pass);
    let mut reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity))?;
    let mut buf = Zeroizing::new(Vec::new());
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Load secret with interactive passphrase prompt and retry logic
pub fn load_and_decrypt_interactive<P: AsRef<Path>>(
    path: P, 
    prompt: Option<&str>
) -> Result<Zeroizing<Vec<u8>>, KeystoreError> {
    let input = PassphraseInput::new();
    let default_prompt = format!("Enter passphrase for {}", path.as_ref().display());
    let prompt = prompt.unwrap_or(&default_prompt);
    
    for attempt in 1..=input.config.max_attempts {
        let passphrase = input.prompt_for_decrypt(&format!("{} (attempt {}/{})", 
                                                          prompt, attempt, input.config.max_attempts))?;
        
        match load_and_decrypt(&path, &passphrase) {
            Ok(secret) => return Ok(secret),
            Err(KeystoreError::Decrypt(_)) | Err(KeystoreError::DecryptFailed) => {
                if attempt < input.config.max_attempts {
                    eprintln!("Incorrect passphrase. Please try again.");
                    continue;
                } else {
                    return Err(KeystoreError::DecryptFailed);
                }
            },
            Err(e) => return Err(e),
        }
    }
    
    Err(KeystoreError::MaxRetriesExceeded)
}

/// Encrypt and store with interactive passphrase prompt
pub fn encrypt_and_store_interactive<P: AsRef<Path>>(
    secret: &Zeroizing<Vec<u8>>, 
    path: P, 
    prompt: Option<&str>
) -> Result<(), KeystoreError> {
    let input = PassphraseInput::new();
    let default_prompt = format!("Enter passphrase to encrypt {}", path.as_ref().display());
    let prompt = prompt.unwrap_or(&default_prompt);
    
    let passphrase = input.prompt_for_encrypt(prompt)?;
    encrypt_and_store(secret, path, &passphrase)
}

/// Load secret if file exists and not older than `max_age`. Otherwise generate via callback `gen` and store.
pub fn load_or_rotate<P, F>(path: P, passphrase: &str, max_age: Duration, gen: F) -> Result<Zeroizing<Vec<u8>>, KeystoreError>
where P: AsRef<Path>, F: Fn() -> Zeroizing<Vec<u8>> {
    let p = path.as_ref();
    let meta = fs::metadata(p);
    let need_new = match meta {
        Ok(m) => {
            if let Ok(modt) = m.modified() {
                modt.elapsed().unwrap_or(Duration::from_secs(0)) > max_age
            } else { true }
        }
        Err(_) => true,
    };

    if need_new {
        let secret = gen();
        encrypt_and_store(&secret, p, passphrase)?;
        Ok(secret)
    } else {
        load_and_decrypt(p, passphrase)
    }
}

/// Load or rotate with interactive passphrase prompts
pub fn load_or_rotate_interactive<P, F>(
    path: P, 
    max_age: Duration, 
    gen: F
) -> Result<Zeroizing<Vec<u8>>, KeystoreError>
where 
    P: AsRef<Path>, 
    F: Fn() -> Zeroizing<Vec<u8>> 
{
    let p = path.as_ref();
    let meta = fs::metadata(p);
    let need_new = match meta {
        Ok(m) => {
            if let Ok(modt) = m.modified() {
                modt.elapsed().unwrap_or(Duration::from_secs(0)) > max_age
            } else { true }
        }
        Err(_) => true,
    };

    if need_new {
        let secret = gen();
        encrypt_and_store_interactive(&secret, p, None)?;
        Ok(secret)
    } else {
        load_and_decrypt_interactive(p, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    fn temp_file(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(name);
        p
    }

    #[test]
    fn roundtrip() {
        let path = temp_file("nyx_keystore_test.age");
        let secret = Zeroizing::new(b"top secret key".to_vec());
        encrypt_and_store(&secret, &path, "hunter2").unwrap();
        let recovered = load_and_decrypt(&path, "hunter2").unwrap();
        assert_eq!(&*secret, &*recovered);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn wrong_passphrase() {
        let path = temp_file("nyx_keystore_test2.age");
        let secret = Zeroizing::new(b"another secret".to_vec());
        encrypt_and_store(&secret, &path, "pass1").unwrap();
        let err = load_and_decrypt(&path, "wrongpass").unwrap_err();
        match err {
            KeystoreError::DecryptFailed | KeystoreError::Encrypt(_) | KeystoreError::Decrypt(_) => {},
            _ => panic!("unexpected error type"),
        }
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rotate_by_age() {
        let path = temp_file("nyx_keystore_rotate.age");
        let secret = Zeroizing::new(b"old".to_vec());
        encrypt_and_store(&secret, &path, "pw").unwrap();
        // Set mtime to old (simulate 100 days)
        #[cfg(unix)] {
            use filetime::FileTime;
            use std::time::SystemTime;
            let ft = FileTime::from_system_time(SystemTime::now() - std::time::Duration::from_secs(86400*100));
            filetime::set_file_mtime(&path, ft).unwrap();
        }
        let new = load_or_rotate(&path, "pw", std::time::Duration::from_secs(86400*30), || Zeroizing::new(b"newsecret".to_vec())).unwrap();
        assert_eq!(&*new, b"newsecret");
        fs::remove_file(&path).unwrap();
    }
    
    #[test]
    fn test_password_strength_assessment() {
        assert_eq!(PasswordStrength::assess("123"), PasswordStrength::VeryWeak);
        assert_eq!(PasswordStrength::assess("password"), PasswordStrength::VeryWeak);
        assert_eq!(PasswordStrength::assess("Password1"), PasswordStrength::Weak);
        assert_eq!(PasswordStrength::assess("Password123!"), PasswordStrength::Good);
        assert_eq!(PasswordStrength::assess("Str0ng!P@ssw0rd#2024"), PasswordStrength::Strong);
    }
    
    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert!(config.show_strength_hints);
        assert!(config.confirm_on_create);
        assert_eq!(config.min_length, 8);
        assert!(!config.allow_empty);
    }
    
    #[test]
    fn test_input_mode_programmatic() {
        let input = PassphraseInput::new()
            .with_mode(InputMode::Programmatic("test_password".to_string()));
        
        let result = input.prompt_for_decrypt("Test prompt");
        assert!(result.is_ok());
        assert_eq!(&*result.unwrap(), "test_password");
    }
    
    #[test]
    fn test_password_validation() {
        let input = PassphraseInput::new();
        
        // Valid password
        assert!(input.validate_password("validpassword123").is_ok());
        
        // Too short
        assert!(input.validate_password("short").is_err());
        
        // Empty (not allowed by default)
        assert!(input.validate_password("").is_err());
        
        // Empty allowed with custom config
        let config = RetryConfig {
            allow_empty: true,
            min_length: 0,
            ..Default::default()
        };
        let input_allow_empty = PassphraseInput::with_config(config);
        assert!(input_allow_empty.validate_password("").is_ok());
    }
    
    #[test]
    fn test_keystore_error_types() {
        // Test that all error types can be created and formatted
        let errors = vec![
            KeystoreError::DecryptFailed,
            KeystoreError::MaxRetriesExceeded,
            KeystoreError::UserCancelled,
            KeystoreError::NoInputAvailable,
        ];
        
        for error in errors {
            let error_string = format!("{}", error);
            assert!(!error_string.is_empty());
        }
    }
    
    #[test]
    fn test_password_strength_descriptions() {
        let strengths = vec![
            PasswordStrength::VeryWeak,
            PasswordStrength::Weak,
            PasswordStrength::Fair,
            PasswordStrength::Good,
            PasswordStrength::Strong,
        ];
        
        for strength in strengths {
            let desc = strength.description();
            assert!(!desc.is_empty());
            
            let color = strength.color_code();
            assert!(color.starts_with("\x1b["));
        }
    }
} 