pub fn xor_encrypt(data: &mut [u8], passphrase: &str) {
    let passphrase = passphrase.as_bytes();
    for (index, byte) in data.iter_mut().enumerate() {
        let pass_index = index % passphrase.len();
        *byte ^= passphrase[pass_index];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_encryption_test() {
        let input = "hello".as_bytes();
        let mut buffer = "hello".as_bytes().to_vec();
        xor_encrypt(&mut buffer, "some_password");
        assert_ne!(buffer, input);

        xor_encrypt(&mut buffer, "some_password");
        assert_eq!(buffer, input);
    }

    #[test]
    fn encryption_decryption_with_different_password_should_fail() {
        let input = "hello".as_bytes();
        let mut buffer = "hello".as_bytes().to_vec();
        xor_encrypt(&mut buffer, "password");
        xor_encrypt(&mut buffer, "another_password");
        assert_ne!(input, buffer);
    }
}
