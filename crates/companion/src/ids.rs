use rand::TryRng;

/// Fills from the operating system RNG. Panics only if the OS has no randomness source.
pub fn fill_random(buf: &mut [u8]) {
    rand::rngs::SysRng.try_fill_bytes(buf).expect("operating system random number generator");
}

pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    fill_random(&mut b);
    b
}

pub fn random_b64(n: usize) -> String {
    crate::crypto::channel::b64(&random_bytes(n))
}

/// Short id for layout objects (keys, pages). Not a secret.
pub fn short() -> String {
    random_b64(6)
}

/// Layout ids: 1-40 chars of letters, digits, '-' and '_'.
pub fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
