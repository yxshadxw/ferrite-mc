//! Throughput of the connection cipher, old loop vs new whole-buffer path.
//!
//! `cargo bench -p ferrite-net --bench cfb8` reports MiB/s per implementation;
//! sizes cover a typical chunk packet (64 KiB) and a full chunk batch (1 MiB).

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use ferrite_net::cfb8::{Cfb8, Cfb8Reference};

const KEY: [u8; 16] = *b"ferrite-cfb8-key";

fn plaintext(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i.wrapping_mul(31) ^ 0x5a) as u8)
        .collect()
}

fn bench_cipher(c: &mut Criterion) {
    println!("cfb8 backend: {}", Cfb8::new(&KEY).backend_name());
    for len in [64 * 1024, 1024 * 1024] {
        let mut group = c.benchmark_group(format!("cfb8/{len}"));
        group.throughput(Throughput::Bytes(len as u64));
        let mut data = plaintext(len);

        group.bench_function("encrypt/reference", |b| {
            let mut cipher = Cfb8Reference::new(&KEY);
            b.iter(|| cipher.apply(black_box(data.as_mut_slice()), false));
        });
        group.bench_function("encrypt/current", |b| {
            let mut cipher = Cfb8::new(&KEY);
            b.iter(|| cipher.encrypt(black_box(data.as_mut_slice())));
        });
        group.bench_function("decrypt/reference", |b| {
            let mut cipher = Cfb8Reference::new(&KEY);
            b.iter(|| cipher.apply(black_box(data.as_mut_slice()), true));
        });
        group.bench_function("decrypt/current", |b| {
            let mut cipher = Cfb8::new(&KEY);
            b.iter(|| cipher.decrypt(black_box(data.as_mut_slice())));
        });
        group.finish();
    }
}

criterion_group!(benches, bench_cipher);
criterion_main!(benches);
