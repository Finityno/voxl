//! Isolated measurement of the exact AES/CTR versions used by rtc-crypto.
//! Public fixed test inputs never leave the process. This is not an SRTP,
//! network, codec, endpoint-resource or energy qualification.

use ctr::cipher::{KeyIvInit,StreamCipher};
use std::{hint::black_box,time::Instant};

type Aes128Ctr=ctr::Ctr128BE<aes::Aes128>;

fn cpu_nanos()->u64 {
    let mut usage=std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes this properly sized output on success.
    assert_eq!(unsafe {libc::getrusage(libc::RUSAGE_SELF,usage.as_mut_ptr())},0);
    // SAFETY: the preceding successful call initialized the complete structure.
    let usage=unsafe {usage.assume_init()};
    let micros=i128::from(usage.ru_utime.tv_sec)*1_000_000+i128::from(usage.ru_utime.tv_usec)
        +i128::from(usage.ru_stime.tv_sec)*1_000_000+i128::from(usage.ru_stime.tv_usec);
    u64::try_from(micros).expect("nonnegative process CPU").checked_mul(1000).expect("CPU clock range")
}

fn known_vector() {
    // NIST SP 800-38A F.5.1, first CTR-AES128.Encrypt block. This independent
    // published ciphertext checks both builds before their timing begins.
    let key=[0x2b,0x7e,0x15,0x16,0x28,0xae,0xd2,0xa6,0xab,0xf7,0x15,0x88,0x09,0xcf,0x4f,0x3c];
    let iv=[0xf0,0xf1,0xf2,0xf3,0xf4,0xf5,0xf6,0xf7,0xf8,0xf9,0xfa,0xfb,0xfc,0xfd,0xfe,0xff];
    let plain=[0x6b,0xc1,0xbe,0xe2,0x2e,0x40,0x9f,0x96,0xe9,0x3d,0x7e,0x11,0x73,0x93,0x17,0x2a];
    let mut bytes=plain;
    let mut cipher=Aes128Ctr::new(&key.into(),&iv.into());cipher.apply_keystream(&mut bytes);
    assert_eq!(bytes,[0x87,0x4d,0x61,0x91,0xb6,0x20,0xe3,0x26,0x1b,0xef,0x68,0x64,0x99,0x0d,0xb6,0xce]);
    let mut cipher=Aes128Ctr::new(&key.into(),&iv.into());cipher.apply_keystream(&mut bytes);
    assert_eq!(bytes,plain);
}

fn payloads(size:usize,count:u64)->u64 {
    let key=[0x42;16];let plain=vec![0x5a;size];let mut bytes=plain.clone();let mut digest=0u64;
    for index in 0..count {
        bytes.copy_from_slice(&plain);
        // Up to 256 blocks per bounded payload: consecutive synthetic packets
        // do not overlap counter ranges. No production key or media is used.
        let iv=(u128::from(index)*256).to_be_bytes();
        let mut cipher=Aes128Ctr::new(black_box(&key).into(),black_box(&iv).into());
        cipher.apply_keystream(black_box(&mut bytes));
        digest=digest.wrapping_mul(31).wrapping_add(u64::from(bytes[0]));
    }
    // Retain a whole final-payload checksum outside timing's per-packet work.
    for byte in black_box(bytes) {digest=digest.wrapping_mul(31).wrapping_add(u64::from(byte));}
    black_box(digest)
}

fn main() {
    let args:Vec<_>=std::env::args().collect();assert_eq!(args.len(),3,"expected payload bytes and packet count");
    let size:usize=args[1].parse().expect("payload size");let count:u64=args[2].parse().expect("packet count");
    assert!((1..=4096).contains(&size) && (1..=2_000_000).contains(&count),"bounded workload required");
    known_vector();let _=payloads(size,1000);
    let cpu=cpu_nanos();let wall=Instant::now();let digest=payloads(size,count);let wall_nanos=wall.elapsed().as_nanos();let cpu_nanos=cpu_nanos()-cpu;
    #[cfg(target_arch="aarch64")]
    let available=std::arch::is_aarch64_feature_detected!("aes");
    #[cfg(not(target_arch="aarch64"))]
    let available=false;
    println!("{{\"vectorPassed\":true,\"payloadBytes\":{size},\"packets\":{count},\"cpuNanos\":{cpu_nanos},\"wallNanos\":{wall_nanos},\"digest\":\"{digest:016x}\",\"aesTypeBytes\":{},\"armAesAvailable\":{available}}}",std::mem::size_of::<aes::Aes128>());
}
