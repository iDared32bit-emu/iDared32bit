/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! CommonCrypto and friends
//!
//! Documented in its [man pages.](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/CC_crypto.3cc.html#//apple_ref/doc/man/3cc/CC_crypto)
//! Also [open source.](https://github.com/apple-oss-distributions/CommonCrypto)
#![allow(non_upper_case_globals)] // Lots of Apple constants begin with "k"

use crate::dyld::FunctionExports;
use crate::mem::{ConstVoidPtr, GuestUSize, MutPtr, MutVoidPtr, SafeRead};
use crate::{export_c_func, Environment};
use aes::cipher::block_padding::{NoPadding, Padding, Pkcs7};
use aes::cipher::{BlockCipher, BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit};
use digest::Digest;
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::Sha1;
use sha2::digest::generic_array::GenericArray;
use sha2::{Sha224, Sha256, Sha384, Sha512};
use std::collections::HashMap;

// TODO: struct definition
#[allow(non_camel_case_types)]
struct CC_MD5_CTX {}

#[derive(Default)]
pub struct State {
    md5_contexts: HashMap<MutPtr<CC_MD5_CTX>, Md5>,
}
impl State {
    fn get_mut(env: &mut Environment) -> &mut Self {
        &mut env.libc_state.crypto
    }
}

fn CC_MD5_Init(env: &mut Environment, ctx: MutPtr<CC_MD5_CTX>) -> i32 {
    log_once!("Warning: CC_MD5_Init doesn't update side effects! (internal changes to CC_MD5_CTX are not done)");
    assert!(!State::get_mut(env).md5_contexts.contains_key(&ctx));
    State::get_mut(env).md5_contexts.insert(ctx, Md5::new());
    1 // success
}

fn CC_MD5_Update(
    env: &mut Environment,
    ctx: MutPtr<CC_MD5_CTX>,
    data: ConstVoidPtr,
    len: u32,
) -> i32 {
    log_once!("Warning: CC_MD5_Update doesn't update side effects! (internal changes to CC_MD5_CTX are not done)");
    let hasher = env.libc_state.crypto.md5_contexts.get_mut(&ctx).unwrap();
    hasher.update(env.mem.bytes_at(data.cast(), len));
    1 // success
}

fn CC_MD5_Final(env: &mut Environment, md: MutPtr<u8>, ctx: MutPtr<CC_MD5_CTX>) -> i32 {
    log_once!("Warning: CC_MD5_Final doesn't update side effects! (internal changes to CC_MD5_CTX are not done)");
    let hasher = State::get_mut(env).md5_contexts.remove(&ctx).unwrap();
    let digest = hasher.finalize();
    env.mem.bytes_at_mut(md, 16).copy_from_slice(&digest[..]);
    1 // success
}

type CCHmacAlgorithm = u32;
pub const kCCHmacAlgSHA1: CCHmacAlgorithm = 0;
pub const kCCHmacAlgMD5: CCHmacAlgorithm = 1;
pub const kCCHmacAlgSHA256: CCHmacAlgorithm = 2;
pub const kCCHmacAlgSHA384: CCHmacAlgorithm = 3;
pub const kCCHmacAlgSHA512: CCHmacAlgorithm = 4;
pub const kCCHmacAlgSHA224: CCHmacAlgorithm = 5;

type CCOperation = u32;
pub const kCCEncrypt: CCOperation = 0;
pub const kCCDecrypt: CCOperation = 1;
type CCAlgorithm = u32;
// Also used for AES of other sizes.
pub const kCCAlgorithmAES128: CCAlgorithm = 0;
type CCOptions = u32;
pub const kCCOptionPKCS7Padding: CCOptions = 1;
pub const kCCOptionECBMode: CCOptions = 2;
type CCCryptorStatus = i32;
pub const kCCSuccess: CCCryptorStatus = 0;
pub const kCCParamError: CCCryptorStatus = -4300;
pub const kCCBufferTooSmall: CCCryptorStatus = -4301;
pub const kCCAlignmentError: CCCryptorStatus = -4303;
pub const kCCDecodeError: CCCryptorStatus = -4304;

fn CC_MD5(env: &mut Environment, data: ConstVoidPtr, len: u32, md: MutPtr<u8>) -> MutPtr<u8> {
    let mut hasher = Md5::new();
    hasher.update(env.mem.bytes_at(data.cast(), len));
    let digest = hasher.finalize();
    env.mem.bytes_at_mut(md, 16).copy_from_slice(&digest[..]);
    md
}

fn CC_SHA1(env: &mut Environment, data: ConstVoidPtr, len: u32, md: MutPtr<u8>) -> MutPtr<u8> {
    let mut hasher = Sha1::new();
    hasher.update(env.mem.bytes_at(data.cast(), len));
    let digest = hasher.finalize();
    env.mem.bytes_at_mut(md, 20).copy_from_slice(&digest[..]);
    md
}

fn CC_SHA256(env: &mut Environment, data: ConstVoidPtr, len: u32, md: MutPtr<u8>) -> MutPtr<u8> {
    let mut hasher = Sha256::new();
    hasher.update(env.mem.bytes_at(data.cast(), len));
    let digest = hasher.finalize();
    env.mem.bytes_at_mut(md, 32).copy_from_slice(&digest[..]);
    md
}

/// All state lives in the app's context, so apps can copy it.
#[allow(non_camel_case_types)]
#[repr(C, packed)]
struct CC_SHA256_CTX {
    /// Length in bits, low word first.
    count: [u32; 2],
    hash: [u32; 8],
    wbuf: [u8; 64],
}
unsafe impl SafeRead for CC_SHA256_CTX {}

impl CC_SHA256_CTX {
    fn len_bits(&self) -> u64 {
        let count = self.count;
        (u64::from(count[1]) << 32) | u64::from(count[0])
    }

    fn update(&mut self, mut data: &[u8]) {
        let len_bits = self.len_bits();
        let mut used = (len_bits / 8 % 64) as usize;
        let len_bits = len_bits.wrapping_add(data.len() as u64 * 8);
        self.count = [len_bits as u32, (len_bits >> 32) as u32];

        let mut hash = self.hash;
        let mut wbuf = self.wbuf;
        while !data.is_empty() {
            let n = (64 - used).min(data.len());
            wbuf[used..used + n].copy_from_slice(&data[..n]);
            used += n;
            data = &data[n..];
            if used == 64 {
                sha2::compress256(&mut hash, &[GenericArray::clone_from_slice(&wbuf)]);
                used = 0;
            }
        }
        self.hash = hash;
        self.wbuf = wbuf;
    }

    fn finish(mut self) -> [u8; 32] {
        let len_bits = self.len_bits();
        let used = (len_bits / 8 % 64) as usize;
        let mut padding = vec![0x80];
        padding.resize(1 + (119 - used) % 64, 0);
        padding.extend_from_slice(&len_bits.to_be_bytes());
        self.update(&padding);
        self.hash
            .map(u32::to_be_bytes)
            .as_flattened()
            .try_into()
            .unwrap()
    }
}

fn CC_SHA256_Init(env: &mut Environment, c: MutPtr<CC_SHA256_CTX>) -> i32 {
    let ctx = CC_SHA256_CTX {
        count: [0; 2],
        hash: [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ],
        wbuf: [0; 64],
    };
    env.mem.write(c, ctx);
    1
}

fn CC_SHA256_Update(
    env: &mut Environment,
    c: MutPtr<CC_SHA256_CTX>,
    data: ConstVoidPtr,
    len: u32,
) -> i32 {
    let mut ctx = env.mem.read(c);
    ctx.update(env.mem.bytes_at(data.cast(), len));
    env.mem.write(c, ctx);
    1
}

fn CC_SHA256_Final(env: &mut Environment, md: MutPtr<u8>, c: MutPtr<CC_SHA256_CTX>) -> i32 {
    let digest = env.mem.read(c).finish();
    env.mem.bytes_at_mut(md, 32).copy_from_slice(&digest);
    1
}

fn hmac<M: Mac + KeyInit>(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <M as Mac>::new_from_slice(key).unwrap();
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn CCHmac(
    env: &mut Environment,
    algorithm: CCHmacAlgorithm,
    key: ConstVoidPtr,
    key_length: GuestUSize,
    data: ConstVoidPtr,
    data_length: GuestUSize,
    mac_out: MutVoidPtr,
) {
    let key = env.mem.bytes_at(key.cast(), key_length);
    let data = env.mem.bytes_at(data.cast(), data_length);
    let res = match algorithm {
        kCCHmacAlgSHA1 => hmac::<Hmac<Sha1>>(key, data),
        kCCHmacAlgMD5 => hmac::<Hmac<Md5>>(key, data),
        kCCHmacAlgSHA256 => hmac::<Hmac<Sha256>>(key, data),
        kCCHmacAlgSHA384 => hmac::<Hmac<Sha384>>(key, data),
        kCCHmacAlgSHA512 => hmac::<Hmac<Sha512>>(key, data),
        kCCHmacAlgSHA224 => hmac::<Hmac<Sha224>>(key, data),
        _ => panic!("Unsupported algorithm {algorithm} for CCHmac(...)!"),
    };
    let out = env
        .mem
        .bytes_at_mut(mac_out.cast(), res.len().try_into().unwrap());
    out.copy_from_slice(&res);
}

/// Encrypts or decrypts with block cipher `C` in ECB or CBC mode, padding
/// with `P`. Returns [None] if the padding is wrong when decrypting.
fn block_crypt<C, P>(
    op: CCOperation,
    ecb: bool,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Option<Vec<u8>>
where
    C: BlockCipher + BlockEncryptMut + BlockDecryptMut + KeyInit,
    P: Padding<C::BlockSize>,
{
    let key = GenericArray::from_slice(key);
    let iv = GenericArray::from_slice(iv);
    Some(match (op, ecb) {
        (kCCEncrypt, true) => ecb::Encryptor::<C>::new(key).encrypt_padded_vec_mut::<P>(data),
        (kCCEncrypt, false) => cbc::Encryptor::<C>::new(key, iv).encrypt_padded_vec_mut::<P>(data),
        (_, true) => ecb::Decryptor::<C>::new(key)
            .decrypt_padded_vec_mut::<P>(data)
            .ok()?,
        (_, false) => cbc::Decryptor::<C>::new(key, iv)
            .decrypt_padded_vec_mut::<P>(data)
            .ok()?,
    })
}

#[allow(clippy::too_many_arguments)]
fn CCCrypt(
    env: &mut Environment,
    op: CCOperation,
    alg: CCAlgorithm,
    options: CCOptions,
    key: ConstVoidPtr,
    key_length: GuestUSize,
    iv: ConstVoidPtr,
    data_in: ConstVoidPtr,
    data_in_length: GuestUSize,
    data_out: MutVoidPtr,
    data_out_available: GuestUSize,
    data_out_moved: MutPtr<GuestUSize>,
) -> CCCryptorStatus {
    assert!(
        alg == kCCAlgorithmAES128,
        "Unsupported algorithm {alg} for CCCrypt(...)!"
    );
    if op != kCCEncrypt && op != kCCDecrypt {
        return kCCParamError;
    }
    if ![16, 24, 32].contains(&key_length) {
        return kCCParamError;
    }
    const BLOCK_SIZE: GuestUSize = 16;
    let padding = options & kCCOptionPKCS7Padding != 0;
    let ecb = options & kCCOptionECBMode != 0;

    // Like CCCryptorGetOutputLength(), the size needed before unpadding.
    let needed = if op == kCCEncrypt && padding {
        (data_in_length / BLOCK_SIZE + 1) * BLOCK_SIZE
    } else {
        data_in_length
    };
    if needed > data_out_available {
        if !data_out_moved.is_null() {
            env.mem.write(data_out_moved, needed);
        }
        return kCCBufferTooSmall;
    }
    if (op == kCCDecrypt || !padding) && !data_in_length.is_multiple_of(BLOCK_SIZE) {
        return kCCAlignmentError;
    }

    // Without an IV, CBC mode uses one of all zeroes.
    let iv = if iv.is_null() {
        [0u8; BLOCK_SIZE as usize]
    } else {
        env.mem.bytes_at(iv.cast(), BLOCK_SIZE).try_into().unwrap()
    };
    let key = env.mem.bytes_at(key.cast(), key_length);
    let data = env.mem.bytes_at(data_in.cast(), data_in_length);
    let result = match (key_length, padding) {
        (16, false) => block_crypt::<aes::Aes128, NoPadding>(op, ecb, key, &iv, data),
        (16, true) => block_crypt::<aes::Aes128, Pkcs7>(op, ecb, key, &iv, data),
        (24, false) => block_crypt::<aes::Aes192, NoPadding>(op, ecb, key, &iv, data),
        (24, true) => block_crypt::<aes::Aes192, Pkcs7>(op, ecb, key, &iv, data),
        (32, false) => block_crypt::<aes::Aes256, NoPadding>(op, ecb, key, &iv, data),
        _ => block_crypt::<aes::Aes256, Pkcs7>(op, ecb, key, &iv, data),
    };
    let Some(result) = result else {
        return kCCDecodeError;
    };
    let result_length: GuestUSize = result.len().try_into().unwrap();
    env.mem
        .bytes_at_mut(data_out.cast(), result_length)
        .copy_from_slice(&result);
    if !data_out_moved.is_null() {
        env.mem.write(data_out_moved, result_length);
    }
    kCCSuccess
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CC_MD5_Init(_)),
    export_c_func!(CC_MD5_Update(_, _, _)),
    export_c_func!(CC_MD5_Final(_, _)),
    export_c_func!(CC_MD5(_, _, _)),
    export_c_func!(CC_SHA1(_, _, _)),
    export_c_func!(CC_SHA256(_, _, _)),
    export_c_func!(CC_SHA256_Init(_)),
    export_c_func!(CC_SHA256_Update(_, _, _)),
    export_c_func!(CC_SHA256_Final(_, _)),
    export_c_func!(CCCrypt(_, _, _, _, _, _, _, _, _, _, _)),
    export_c_func!(CCHmac(_, _, _, _, _, _)),
];
