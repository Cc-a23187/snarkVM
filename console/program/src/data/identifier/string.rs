// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use super::*;

impl<N: Network> FromStr for Identifier<N> {
    type Err = Error;

    /// Reads in an identifier from a string.
    fn from_str(identifier: &str) -> Result<Self, Self::Err> {
        // Ensure the identifier is not an empty string, and does not start with a number.
        match identifier.chars().next() {
            Some(character) => ensure!(!character.is_numeric(), "Identifier cannot start with a number"),
            None => bail!("Identifier cannot be an empty string"),
        }

        // Ensure the identifier is alphanumeric and underscores.
        if !identifier.chars().all(|character| character.is_alphanumeric() || character == '_') {
            bail!("Identifier must be alphanumeric and underscores")
        }

        // Ensure the identifier is not solely underscores.
        if identifier.chars().all(|character| character == '_') {
            bail!("Identifier cannot consist solely of underscores")
        }

        // Ensure identifier fits within the data capacity of the base field.
        let max_bytes = N::Field::size_in_data_bits() / 8; // Note: This intentionally rounds down.
        if identifier.len() > max_bytes {
            bail!("Identifier is too large. Identifiers must be <= {max_bytes} bytes long")
        }

        // Note: The string bytes themselves are **not** little-endian. Rather, they are order-preserving
        // for reconstructing the string when recovering the field element back into bytes.
        Ok(Self(N::field_from_bits_le(&identifier.as_bytes().to_bits_le())?, identifier.len() as u8))
    }
}

impl<N: Network> fmt::Display for Identifier<N> {
    /// Prints the identifier as a string.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Convert the identifier to bits.
        let bits_le = self.0.to_bits_le();

        // Convert the bits to bytes.
        let bytes = bits_le
            .chunks(8)
            .map(|byte| u8::from_bits_le(byte).map_err(|_| fmt::Error))
            .collect::<Result<Vec<u8>, _>>()?;

        // Parse the bytes as a UTF-8 string.
        let string = String::from_utf8(bytes).map_err(|_| fmt::Error)?;

        // Truncate the UTF-8 string at the first instance of '\0'.
        match string.split('\0').next() {
            // Check that the UTF-8 string matches the expected length.
            Some(string) => match string.len() == self.1 as usize {
                // Return the string.
                true => write!(f, "{string}"),
                false => Err(fmt::Error),
            },
            None => Err(fmt::Error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::identifier::tests::sample_identifier_as_string;
    use snarkvm_console_network::Testnet3;

    type CurrentNetwork = Testnet3;

    const ITERATIONS: usize = 100;

    #[test]
    fn test_identifier_from_str() -> Result<()> {
        let candidate = Identifier::<CurrentNetwork>::from_str("foo_bar").unwrap();
        assert_eq!("foo_bar", candidate.to_string());

        for _ in 0..ITERATIONS {
            // Sample a random fixed-length alphanumeric string, that always starts with an alphabetic character.
            let expected_string = sample_identifier_as_string::<CurrentNetwork>()?;
            // Recover the field element from the bits.
            let expected_field = <CurrentNetwork as Network>::field_from_bits_le(&expected_string.to_bits_le())?;

            let candidate = Identifier::<CurrentNetwork>::from_str(&expected_string)?;
            assert_eq!(expected_string, candidate.to_string());
            assert_eq!(expected_field, candidate.0);
            assert_eq!(expected_string.len(), candidate.1 as usize);
        }
        Ok(())
    }

    #[test]
    fn test_identifier_from_str_fails() {
        // Must be alphanumeric or underscore.
        assert!(Identifier::<CurrentNetwork>::from_str("foo_bar~baz").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("foo_bar-baz").is_err());

        // Must not be solely underscores.
        assert!(Identifier::<CurrentNetwork>::from_str("_").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("__").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("___").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("____").is_err());

        // Must not start with a number.
        assert!(Identifier::<CurrentNetwork>::from_str("1").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("2").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("3").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("1foo").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("12").is_err());
        assert!(Identifier::<CurrentNetwork>::from_str("111").is_err());

        // Must fit within the data capacity of a base field element.
        let identifier = Identifier::<CurrentNetwork>::from_str(
            "foo_bar_baz_qux_quux_quuz_corge_grault_garply_waldo_fred_plugh_xyzzy",
        );
        assert!(identifier.is_err());
    }

    #[test]
    fn test_identifier_display() -> Result<()> {
        let identifier = Identifier::<CurrentNetwork>::from_str("foo_bar")?;
        assert_eq!("foo_bar", format!("{identifier}"));
        Ok(())
    }
}
