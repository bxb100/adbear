use rand::prelude::*;

// https://cs.android.com/android-studio/platform/tools/adt/idea/+/mirror-goog-studio-main:android-adb/src/com/android/tools/idea/adb/wireless/WiFiPairingServiceImpl.kt;l=1;bpv=0
const ALL_CHARACTERS: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-+*/<>{}";

pub fn generate() -> String {
    let mut rng = rand::rng();
    let data = (0..20)
        .map(|_| {
            let idx = rng.random_range(0..ALL_CHARACTERS.len());
            ALL_CHARACTERS[idx]
        })
        .collect::<Vec<_>>();

    unsafe { String::from_utf8_unchecked(data) }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_generate() {
        let password = super::generate();
        println!("Generated password: {}", password);
        assert_eq!(password.len(), 20);
    }
}
