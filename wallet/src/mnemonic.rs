//! # BIP39 Mnemonic and Seed generation
//!
//! Example
//!
//! ```
//! # use witnet_wallet::mnemonic::MnemonicGen;
//! let mnemonic = MnemonicGen::new().generate();
//!
//! // A Mnemonic Seed must be protected by a passphrase
//! let passphrase = "";
//!
//! // String of mnemonic words
//! let words = mnemonic.words();
//! // Seed that can be used to generate a master secret key
//! let seed = mnemonic.seed(passphrase);
//! ```
use bip39;

/// BIP39 Mnemonic
pub struct Mnemonic(bip39::Mnemonic);

impl Mnemonic {
    /// Get the list of mnemonic words
    pub fn words(&self) -> &str {
        self.0.phrase()
    }

    /// Get the binary seed used for generating a master secret key
    pub fn seed<'a, S: Into<&'a str>>(&self, passphrase: S) -> Seed {
        Seed(bip39::Seed::new(&self.0, passphrase.into()))
    }
}

/// BIP39 Seed generated from a Mnemonic
pub struct Seed(bip39::Seed);

impl Seed {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl AsRef<[u8]> for Seed {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// Number of words of the Mnemonic
///
/// The number of words of the Mnemonic is proportional to the
/// entropy:
///
/// * `128 bits` generates `12 words` mnemonic
/// * `160 bits` generates `15 words` mnemonic
/// * `192 bits` generates `18 words` mnemonic
/// * `224 bits` generates `21 words` mnemonic
/// * `256 bits` generates `24 words` mnemonic
#[derive(Debug, PartialEq)]
pub enum Length {
    Words12,
    Words15,
    Words18,
    Words21,
    Words24,
}

/// The language in which Mnemonics are generated
#[derive(Debug, PartialEq)]
pub enum Lang {
    English,
}

/// BIP39 Mnemonic generator
pub struct MnemonicGen {
    len: Length,
    lang: Lang,
}

impl MnemonicGen {
    /// Create a new BIP39 Mnemonic generator
    pub fn new() -> Self {
        MnemonicGen {
            len: Length::Words12,
            lang: Lang::English,
        }
    }

    /// Set how many words the Mnemonic should have
    pub fn with_len(mut self, len: Length) -> Self {
        self.len = len;
        self
    }

    /// Set which language to use in the Mnemonic words
    pub fn with_lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    /// Consume this generator and return the BIP39 Mnemonic
    pub fn generate(self) -> Mnemonic {
        let mnemonic_type = match self.len {
            Length::Words12 => bip39::MnemonicType::Words12,
            Length::Words15 => bip39::MnemonicType::Words15,
            Length::Words18 => bip39::MnemonicType::Words18,
            Length::Words21 => bip39::MnemonicType::Words21,
            Length::Words24 => bip39::MnemonicType::Words24,
        };
        let lang = match self.lang {
            Lang::English => bip39::Language::English,
        };
        let mnemonic = bip39::Mnemonic::new(mnemonic_type, lang);

        Mnemonic(mnemonic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_default() {
        let gen = MnemonicGen::new();

        assert_eq!(gen.len, Length::Words12);
        assert_eq!(gen.lang, Lang::English);
    }

    #[test]
    fn test_gen_with_len() {
        let gen = MnemonicGen::new().with_len(Length::Words24);

        assert_eq!(gen.len, Length::Words24);
        assert_eq!(gen.lang, Lang::English);
    }

    #[test]
    fn test_generate() {
        let mnemonic = MnemonicGen::new().generate();
        let words: Vec<&str> = mnemonic.words().split_whitespace().collect();

        assert_eq!(words.len(), 12);
    }

    #[test]
    fn test_seed_as_ref() {
        let mnemonic = MnemonicGen::new().generate();
        let seed = mnemonic.seed("");
        let bytes: &[u8] = seed.as_ref();

        assert_eq!(bytes, seed.as_bytes());
    }
}
