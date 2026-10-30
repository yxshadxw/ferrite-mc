//! AES-128-CFB8, the stream cipher Minecraft switches to once a connection is
//! encrypted (`new Cipher("AES/CFB8/NoPadding")` in vanilla).
//!
//! CFB8 feeds one byte of ciphertext back into the 16-byte shift register after
//! every byte, so the naive implementation encrypts a fresh block per byte:
//! clone the register into a block buffer, call the AES block cipher, xor one
//! byte, then memmove the register forward. Recorded here as
//! [`Cfb8Reference`], that loop spends most of its time outside AES: the
//! register copy, the runtime backend dispatch and the 15-byte shift are all
//! per-byte overhead.
//!
//! [`Cfb8`] keeps the register in a vector register for the whole buffer and
//! moves the feedback in with a single byte insert. It mirrors how the native
//! ciphers in Velocity and Krypton are used:
//!
//! * Velocity (<https://github.com/PaperMC/Velocity>, GPL-3.0) implements
//!   whole-buffer, in-place encryption in `native/src/main/c/jni_cipher_openssl.c`
//!   behind
//!   `native/src/main/java/com/velocitypowered/natives/encryption/OpenSslCipherImpl.java`
//!   (`VelocityCipher::process`), with `JavaVelocityCipher` as the fallback.
//! * Krypton (<https://github.com/astei/krypton>, LGPL-3.0) swaps that cipher in
//!   for the JDK's `AES/CFB8` in
//!   `src/main/java/me/steinborn/krypton/mod/shared/network/pipeline/MinecraftCipherEncoder.java`
//!   and `MinecraftCipherDecoder.java`, so a packet is processed in one call
//!   instead of byte by byte.
//!
//! Both are GPL-compatible. This module is an independent Rust implementation of
//! the same technique — no code was copied — using AES-NI intrinsics with
//! runtime detection and the `aes` crate as the software fallback.
//!
//! Decryption is four-way interleaved: in CFB8 the shift register is just the
//! last 16 ciphertext bytes, so four consecutive states can be formed without
//! waiting for the AES results, which hides the block cipher's latency.
//! Encryption cannot be parallelised this way (each state contains the byte we
//! have not produced yet) and stays a single AES block per byte.

use aes::{
    Aes128,
    cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray},
};

#[cfg(target_arch = "x86_64")]
mod ni {
    //! AES-NI backend: the key schedule is expanded with `aeskeygenassist` and
    //! the CFB8 feedback is a single `palignr`.

    use core::arch::x86_64::*;

    /// Eleven expanded AES-128 round keys.
    pub(crate) struct RoundKeys {
        rk: [__m128i; 11],
    }

    /// One AES-128 block encryption of `$block` under `$rk: [__m128i; 11]`.
    macro_rules! aes_encrypt {
        ($rk:expr, $block:expr) => {{
            let block = _mm_xor_si128($block, $rk[0]);
            let block = _mm_aesenc_si128(block, $rk[1]);
            let block = _mm_aesenc_si128(block, $rk[2]);
            let block = _mm_aesenc_si128(block, $rk[3]);
            let block = _mm_aesenc_si128(block, $rk[4]);
            let block = _mm_aesenc_si128(block, $rk[5]);
            let block = _mm_aesenc_si128(block, $rk[6]);
            let block = _mm_aesenc_si128(block, $rk[7]);
            let block = _mm_aesenc_si128(block, $rk[8]);
            let block = _mm_aesenc_si128(block, $rk[9]);
            _mm_aesenclast_si128(block, $rk[10])
        }};
    }

    impl RoundKeys {
        /// Expand `key` into round keys.
        ///
        /// # Safety
        /// The caller must have checked that the CPU supports AES-NI.
        #[target_feature(enable = "aes")]
        pub(crate) unsafe fn new(key: &[u8; 16]) -> Self {
            // SAFETY: AES-NI is available (see the caller), and the register
            // operations only touch locals of the right size.
            unsafe {
                let mut rk = [_mm_setzero_si128(); 11];
                rk[0] = _mm_loadu_si128(key.as_ptr().cast());
                macro_rules! expand {
                    ($rk:ident, $round:literal, $rcon:literal) => {{
                        // g = SubWord(RotWord(w3)) ^ rcon, broadcast to all words.
                        let g = _mm_aeskeygenassist_si128::<$rcon>($rk[$round - 1]);
                        let g = _mm_shuffle_epi32::<0xff>(g);
                        // w0..w3 ^= (w0..w3 << 1 word) ^ (w0..w3 << 2 words).
                        let mut words = $rk[$round - 1];
                        words = _mm_xor_si128(words, _mm_slli_si128::<4>(words));
                        words = _mm_xor_si128(words, _mm_slli_si128::<8>(words));
                        $rk[$round] = _mm_xor_si128(words, g);
                    }};
                }
                expand!(rk, 1, 0x01);
                expand!(rk, 2, 0x02);
                expand!(rk, 3, 0x04);
                expand!(rk, 4, 0x08);
                expand!(rk, 5, 0x10);
                expand!(rk, 6, 0x20);
                expand!(rk, 7, 0x40);
                expand!(rk, 8, 0x80);
                expand!(rk, 9, 0x1b);
                expand!(rk, 10, 0x36);
                Self { rk }
            }
        }

        /// Encrypt `data` in place, carrying the CFB8 shift register in `state`.
        ///
        /// # Safety
        /// The caller must have checked AES-NI and SSSE3 support.
        #[target_feature(enable = "aes,ssse3")]
        pub(crate) unsafe fn encrypt(&self, state: &mut [u8; 16], data: &mut [u8]) {
            // SAFETY: AES-NI and SSSE3 are available (see the caller); the
            // loads/stores stay inside `state` and `data`.
            unsafe {
                let mut register = _mm_loadu_si128(state.as_ptr().cast());
                for byte in data.iter_mut() {
                    // Only the first keystream byte is used, but the whole
                    // block has to be computed.
                    let keystream = _mm_cvtsi128_si32(aes_encrypt!(&self.rk, register)) as u8;
                    let ciphertext = *byte ^ keystream;
                    *byte = ciphertext;
                    // register[1..16] ++ ciphertext, in one instruction.
                    let feedback = _mm_cvtsi32_si128(ciphertext.into());
                    register = _mm_alignr_epi8::<1>(feedback, register);
                }
                _mm_storeu_si128(state.as_mut_ptr().cast(), register);
            }
        }

        /// Decrypt `data` in place, carrying the CFB8 shift register in `state`.
        ///
        /// # Safety
        /// The caller must have checked AES-NI and SSSE3 support.
        #[target_feature(enable = "aes,ssse3")]
        pub(crate) unsafe fn decrypt(&self, state: &mut [u8; 16], data: &mut [u8]) {
            // SAFETY: AES-NI and SSSE3 are available (see the caller); the
            // loads/stores stay inside `state` and `data`.
            unsafe {
                let mut register = _mm_loadu_si128(state.as_ptr().cast());
                let mut at = 0;
                // In CFB8 decryption the register is exactly the last 16
                // ciphertext bytes, so four states can be built from the
                // ciphertext alone and their block encryptions overlap.
                while at + 4 <= data.len() {
                    let input =
                        u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
                    // Lanes 0..3 are the next four ciphertext bytes.
                    let next = _mm_cvtsi32_si128(input as i32);
                    let state0 = register;
                    let state1 = _mm_alignr_epi8::<1>(next, register);
                    let state2 = _mm_alignr_epi8::<2>(next, register);
                    let state3 = _mm_alignr_epi8::<3>(next, register);
                    let ks0 = _mm_cvtsi128_si32(aes_encrypt!(&self.rk, state0)) as u32;
                    let ks1 = _mm_cvtsi128_si32(aes_encrypt!(&self.rk, state1)) as u32;
                    let ks2 = _mm_cvtsi128_si32(aes_encrypt!(&self.rk, state2)) as u32;
                    let ks3 = _mm_cvtsi128_si32(aes_encrypt!(&self.rk, state3)) as u32;
                    let keystream = (ks0 & 0xff)
                        | ((ks1 & 0xff) << 8)
                        | ((ks2 & 0xff) << 16)
                        | ((ks3 & 0xff) << 24);
                    data[at..at + 4].copy_from_slice(&(input ^ keystream).to_le_bytes());
                    // register[4..16] ++ ciphertext[0..4].
                    register = _mm_alignr_epi8::<4>(next, register);
                    at += 4;
                }
                for byte in &mut data[at..] {
                    let input = *byte;
                    let keystream = _mm_cvtsi128_si32(aes_encrypt!(&self.rk, register)) as u8;
                    *byte = input ^ keystream;
                    let feedback = _mm_cvtsi32_si128(input.into());
                    register = _mm_alignr_epi8::<1>(feedback, register);
                }
                _mm_storeu_si128(state.as_mut_ptr().cast(), register);
            }
        }

        /// Encrypt a single block; used by the FIPS-197 known-answer test.
        #[cfg(test)]
        #[target_feature(enable = "aes")]
        pub(crate) unsafe fn encrypt_block(&self, block: [u8; 16]) -> [u8; 16] {
            // SAFETY: AES-NI is available (see the caller); the loads/stores
            // stay inside the 16-byte arrays.
            unsafe {
                let out = aes_encrypt!(&self.rk, _mm_loadu_si128(block.as_ptr().cast()));
                let mut bytes = [0u8; 16];
                _mm_storeu_si128(bytes.as_mut_ptr().cast(), out);
                bytes
            }
        }
    }
}

/// Software AES-128 from the `aes` crate (which itself uses AES-NI when the
/// runtime probe inside that crate finds it).
pub(crate) struct SoftKey {
    cipher: Aes128,
}

impl SoftKey {
    pub(crate) fn new(key: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128::new(GenericArray::from_slice(key)),
        }
    }

    pub(crate) fn encrypt(&self, state: &mut [u8; 16], data: &mut [u8]) {
        self.apply(state, data, false);
    }

    pub(crate) fn decrypt(&self, state: &mut [u8; 16], data: &mut [u8]) {
        self.apply(state, data, true);
    }

    fn apply(&self, state: &mut [u8; 16], data: &mut [u8], decrypt: bool) {
        // Big-endian: the most significant byte is the next keystream source,
        // and `<< 8` shifts the register exactly like CFB8 requires.
        let mut register = u128::from_be_bytes(*state);
        for byte in data.iter_mut() {
            let input = *byte;
            let mut block = GenericArray::from(register.to_be_bytes());
            self.cipher.encrypt_block(&mut block);
            *byte = input ^ block[0];
            let feedback = if decrypt { input } else { *byte };
            register = (register << 8) | u128::from(feedback);
        }
        *state = register.to_be_bytes();
    }
}

/// AES-128-CFB8 with a whole-buffer, in-place interface.
pub struct Cfb8 {
    backend: Backend,
    state: [u8; 16],
}

enum Backend {
    #[cfg(target_arch = "x86_64")]
    AesNi(ni::RoundKeys),
    // The `aes` crate's software schedule is large; keep it out of `Wire`.
    Software(Box<SoftKey>),
}

/// `true` when the CPU can run the AES-NI backend. SSSE3 always accompanies
/// AES-NI in practice, but both are required for `palignr`.
#[cfg(target_arch = "x86_64")]
fn aes_ni_available() -> bool {
    std::arch::is_x86_feature_detected!("aes") && std::arch::is_x86_feature_detected!("ssse3")
}

impl Cfb8 {
    /// Create the cipher. Minecraft uses the shared secret as the IV as well.
    pub fn new(key: &[u8; 16]) -> Self {
        Self::build(key, key)
    }

    fn build(key: &[u8; 16], iv: &[u8; 16]) -> Self {
        #[cfg(target_arch = "x86_64")]
        if aes_ni_available() {
            // SAFETY: the same probes guard the constructor.
            let backend = Backend::AesNi(unsafe { ni::RoundKeys::new(key) });
            return Self {
                backend,
                state: *iv,
            };
        }
        Self {
            backend: Backend::Software(Box::new(SoftKey::new(key))),
            state: *iv,
        }
    }

    /// Only for tests that need a non-key IV.
    #[cfg(test)]
    pub(crate) fn with_iv(key: &[u8; 16], iv: &[u8; 16]) -> Self {
        Self::build(key, iv)
    }

    /// Encrypt `data` in place, continuing the stream.
    pub fn encrypt(&mut self, data: &mut [u8]) {
        match &self.backend {
            // SAFETY: the backend was only constructed after checking the
            // features it requires.
            #[cfg(target_arch = "x86_64")]
            Backend::AesNi(keys) => unsafe { keys.encrypt(&mut self.state, data) },
            Backend::Software(keys) => keys.encrypt(&mut self.state, data),
        }
    }

    /// Decrypt `data` in place, continuing the stream.
    pub fn decrypt(&mut self, data: &mut [u8]) {
        match &self.backend {
            // SAFETY: the backend was only constructed after checking the
            // features it requires.
            #[cfg(target_arch = "x86_64")]
            Backend::AesNi(keys) => unsafe { keys.decrypt(&mut self.state, data) },
            Backend::Software(keys) => keys.decrypt(&mut self.state, data),
        }
    }

    /// Which implementation is in use: `"aes-ni"` or `"software"`.
    pub fn backend_name(&self) -> &'static str {
        match &self.backend {
            #[cfg(target_arch = "x86_64")]
            Backend::AesNi(_) => "aes-ni",
            Backend::Software(_) => "software",
        }
    }

    pub(crate) fn state(&self) -> [u8; 16] {
        self.state
    }

    pub(crate) fn restore(&mut self, state: [u8; 16]) {
        self.state = state;
    }
}

/// The original byte-at-a-time loop, kept as the differential-test oracle and
/// as the baseline for the `cfb8` benchmark.
#[doc(hidden)]
pub struct Cfb8Reference {
    cipher: Aes128,
    shift: [u8; 16],
}

impl Cfb8Reference {
    #[doc(hidden)]
    pub fn new(key: &[u8; 16]) -> Self {
        Self::with_iv(key, key)
    }

    #[doc(hidden)]
    pub fn with_iv(key: &[u8; 16], iv: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128::new(GenericArray::from_slice(key)),
            shift: *iv,
        }
    }

    #[doc(hidden)]
    pub fn apply(&mut self, data: &mut [u8], decrypt: bool) {
        for byte in data {
            let input = *byte;
            let mut block = GenericArray::clone_from_slice(&self.shift);
            self.cipher.encrypt_block(&mut block);
            *byte ^= block[0];
            self.shift.copy_within(1.., 0);
            self.shift[15] = if decrypt { input } else { *byte };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng, rngs::StdRng};

    // NIST SP 800-38A, F.3.7 CFB8-AES128.Encrypt.
    const NIST_KEY: [u8; 16] = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f,
        0x3c,
    ];
    const NIST_IV: [u8; 16] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ];

    #[test]
    fn nist_cfb8_vector() {
        let mut plaintext = [0x6b, 0xc1, 0xbe, 0xe2];
        let mut reference = Cfb8Reference::with_iv(&NIST_KEY, &NIST_IV);
        reference.apply(&mut plaintext, false);
        assert_eq!(plaintext, [0x3b, 0x79, 0x42, 0x4c]);

        let mut production = Cfb8::with_iv(&NIST_KEY, &NIST_IV);
        let mut data = [0x6b, 0xc1, 0xbe, 0xe2];
        production.encrypt(&mut data);
        assert_eq!(data, [0x3b, 0x79, 0x42, 0x4c]);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn aes_ni_matches_fips197() {
        if !aes_ni_available() {
            return;
        }
        let key: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        // FIPS-197 C.1: 00112233445566778899aabbccddeeff -> 69c4e0d8....
        let plaintext: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let expected: [u8; 16] = [
            0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30, 0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4,
            0xc5, 0x5a,
        ];
        // SAFETY: AES-NI was just detected.
        let keys = unsafe { ni::RoundKeys::new(&key) };
        // SAFETY: as above.
        let encrypted = unsafe { keys.encrypt_block(plaintext) };
        assert_eq!(encrypted, expected);
    }

    /// The production cipher must produce exactly the reference bytes for any
    /// key, buffer length and split, in both directions.
    #[test]
    fn matches_reference_over_random_inputs() {
        let mut rng = StdRng::seed_from_u64(0x5eed_cfb8);
        for case in 0..64 {
            let key: [u8; 16] = rng.r#gen();
            let len = rng.gen_range(0..=1500);
            let plaintext: Vec<u8> = (0..len).map(|_| rng.r#gen::<u8>()).collect();

            let mut reference = Cfb8Reference::new(&key);
            let mut expected = plaintext.clone();
            reference.apply(&mut expected, false);

            // One-shot.
            let mut cipher = Cfb8::new(&key);
            let mut actual = plaintext.clone();
            cipher.encrypt(&mut actual);
            assert_eq!(actual, expected, "encrypt case {case}");

            // Random chunk boundaries must not change the stream.
            let mut chunked = Cfb8::new(&key);
            let mut split = plaintext.clone();
            let mut at = 0;
            while at < len {
                let take = rng.gen_range(1..=17).min(len - at);
                chunked.encrypt(&mut split[at..at + take]);
                at += take;
            }
            assert_eq!(split, expected, "chunked encrypt case {case}");

            // Decryption, one-shot and split.
            let mut decryptor = Cfb8::new(&key);
            let mut decrypted = expected.clone();
            decryptor.decrypt(&mut decrypted);
            assert_eq!(decrypted, plaintext, "decrypt case {case}");

            let mut decryptor = Cfb8::new(&key);
            let mut decrypted = expected.clone();
            let mut at = 0;
            while at < len {
                let take = rng.gen_range(1..=17).min(len - at);
                decryptor.decrypt(&mut decrypted[at..at + take]);
                at += take;
            }
            assert_eq!(decrypted, plaintext, "chunked decrypt case {case}");

            // The software fallback must agree too.
            let soft = SoftKey::new(&key);
            let mut state = key;
            let mut soft_encrypted = plaintext.clone();
            soft.encrypt(&mut state, &mut soft_encrypted);
            assert_eq!(soft_encrypted, expected, "soft encrypt case {case}");

            let mut soft_state = key;
            let mut soft_decrypted = expected.clone();
            soft.decrypt(&mut soft_state, &mut soft_decrypted);
            assert_eq!(soft_decrypted, plaintext, "soft decrypt case {case}");
        }
    }
}
