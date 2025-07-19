use criterion::{criterion_group, criterion_main, Criterion};
use std::net::UdpSocket;

#[cfg(target_os = "linux")]
use nix::sys::socket::{sendmmsg, MsgFlags, SockaddrIn};
#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;

/// Benchmark UDP send performance for single vs batch (sendmmsg) send.
fn bench_udp_send(c: &mut Criterion) {
    // Prepare receiver socket on localhost.
    let recv_sock = UdpSocket::bind("127.0.0.1:0").expect("bind recv");
    recv_sock
        .set_nonblocking(true)
        .expect("nonblocking");
    let dst = recv_sock.local_addr().expect("addr");

    // Drain task to avoid RX buffer overflow during benchmark.
    std::thread::spawn(move || {
        let mut buf = [0u8; 1500];
        while let Ok(_) = recv_sock.recv(&mut buf) {}
    });

    // Sender socket.
    let send_sock = UdpSocket::bind("127.0.0.1:0").expect("bind send");
    let payload = [0u8; 1200];

    // Single datagram send benchmark.
    c.bench_function("udp_send_to", |b| {
        b.iter(|| {
            send_sock
                .send_to(&payload, dst)
                .expect("send_to failed");
        })
    });

    // Batch send benchmark using sendmmsg (Linux only).
    #[cfg(target_os = "linux")]
    {
        use nix::sys::socket::InetAddr;
        let fd = send_sock.as_raw_fd();
        let sockaddr = SockaddrIn::from(InetAddr::from_std(&dst));
        c.bench_function("udp_send_mmsg_batch8", |b| {
            b.iter(|| {
                // Prepare array of (&[u8], &SockaddrIn) tuples.
                let bufs = [
                    (&payload[..], &sockaddr),
                    (&payload[..], &sockaddr),
                    (&payload[..], &sockaddr),
                    (&payload[..], &sockaddr),
                    (&payload[..], &sockaddr),
                    (&payload[..], &sockaddr),
                    (&payload[..], &sockaddr),
                    (&payload[..], &sockaddr),
                ];
                // Safe: fd is valid and we ignore errno for brevity.
                let _ = sendmmsg(fd, &bufs, MsgFlags::empty()).expect("sendmmsg");
            })
        });
    }
}

criterion_group!(benches, bench_udp_send);
criterion_main!(benches); 