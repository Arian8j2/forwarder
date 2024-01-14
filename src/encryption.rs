#[inline]
pub fn xor_small_chunk(mut data: Vec<u8>, passphrase: &str) -> Vec<u8> {
    for (index, byte) in data.iter_mut().enumerate() {
        let pass_index = index % passphrase.len();
        *byte ^= passphrase.as_bytes()[pass_index];
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_test() {
        let input = "salam".as_bytes();
        let encrypted = xor_small_chunk(input.to_vec(), "password");
        let decrypted = xor_small_chunk(encrypted, "password");
        assert_eq!(input, decrypted);
    }

    #[test]
    fn encryption_decryption_with_different_password_should_fail() {
        let input = "salam".as_bytes();
        let encrypted = xor_small_chunk(input.to_vec(), "password");
        let decrypted = xor_small_chunk(encrypted, "another_password");
        assert_ne!(input, decrypted);
    }
}
