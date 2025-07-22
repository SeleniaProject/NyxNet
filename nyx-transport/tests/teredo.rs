use std::net::{Ipv4Addr};
use nyx_transport::teredo::TeredoAddr;

#[test]
fn teredo_address_encoding() {
    let server_ip = Ipv4Addr::new(192, 0, 2, 1);
    let client_ip = Ipv4Addr::new(203, 0, 113, 4);
    let port: u16 = 40000; // arbitrary
    let teredo = TeredoAddr::new(server_ip, client_ip, port, false);
    let seg = teredo.addr().segments();

    // Verify service prefix 2001:0000::/32
    assert_eq!(seg[0], 0x2001);
    assert_eq!(seg[1], 0x0000);

    // Verify server IPv4 embedding
    assert_eq!(seg[2], ((server_ip.octets()[0] as u16) << 8) | server_ip.octets()[1] as u16);
    assert_eq!(seg[3], ((server_ip.octets()[2] as u16) << 8) | server_ip.octets()[3] as u16);

    // Flags should be 0 since cone=false
    assert_eq!(seg[4], 0x0000);

    // Obfuscated port should be ones-complement
    assert_eq!(seg[5], !port);

    // Obfuscated client IP verification
    let obsc_ip = [!client_ip.octets()[0], !client_ip.octets()[1], !client_ip.octets()[2], !client_ip.octets()[3]];
    assert_eq!(seg[6], ((obsc_ip[0] as u16) << 8) | obsc_ip[1] as u16);
    assert_eq!(seg[7], ((obsc_ip[2] as u16) << 8) | obsc_ip[3] as u16);
} 