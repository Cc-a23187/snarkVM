// Copyright (C) 2019-2021 Aleo Systems Inc.
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

use crate::prelude::*;
use snarkvm_algorithms::prelude::*;
use snarkvm_utilities::{to_bytes_le, FromBytes, ToBytes, UniformRand};

use anyhow::Result;
use rand::{CryptoRng, Rng};
use std::{
    fmt,
    io::{Read, Result as IoResult, Write},
    str::FromStr,
};

type EncryptedRecordHash<C> = <<C as Parameters>::EncryptedRecordCRH as CRH>::Output;
type EncryptedRecordRandomizer<C> = <<C as Parameters>::AccountEncryptionScheme as EncryptionScheme>::Randomness;
type ProgramCommitment<C> = <<C as Parameters>::ProgramCommitmentScheme as CommitmentScheme>::Output;
type ProgramCommitmentRandomness<C> = <<C as Parameters>::ProgramCommitmentScheme as CommitmentScheme>::Randomness;

/// The transaction authorization contains caller signatures and is required to
/// produce the final transaction proof.
#[derive(Derivative)]
#[derivative(
    Clone(bound = "C: Parameters"),
    Debug(bound = "C: Parameters"),
    PartialEq(bound = "C: Parameters"),
    Eq(bound = "C: Parameters")
)]
pub struct TransactionAuthorization<C: Parameters> {
    pub kernel: TransactionKernel<C>,
    pub input_records: Vec<Record<C>>,
    pub output_records: Vec<Record<C>>,
    pub signatures: Vec<C::AccountSignature>,
    pub noop_compute_keys: Vec<Option<ComputeKey<C>>>,
}

impl<C: Parameters> TransactionAuthorization<C> {
    #[inline]
    pub fn from(state: &StateTransition<C>, signatures: Vec<C::AccountSignature>) -> Self {
        debug_assert!(state.kernel().is_valid());
        debug_assert_eq!(C::NUM_INPUT_RECORDS, state.input_records().len());
        debug_assert_eq!(C::NUM_OUTPUT_RECORDS, state.output_records().len());
        debug_assert_eq!(C::NUM_INPUT_RECORDS, signatures.len());

        Self {
            kernel: state.kernel().clone(),
            input_records: state.input_records().clone(),
            output_records: state.output_records().clone(),
            signatures,
            noop_compute_keys: state.to_noop_compute_keys().clone(),
        }
    }

    #[inline]
    pub fn to_local_data<R: Rng + CryptoRng>(&self, rng: &mut R) -> Result<LocalData<C>> {
        Ok(LocalData::new(
            &self.kernel,
            &self.input_records,
            &self.output_records,
            rng,
        )?)
    }

    #[inline]
    pub fn to_program_commitment<R: Rng + CryptoRng>(
        &self,
        rng: &mut R,
    ) -> Result<(ProgramCommitment<C>, ProgramCommitmentRandomness<C>)> {
        let program_ids = self
            .input_records
            .iter()
            .chain(self.output_records.iter())
            .take(C::NUM_TOTAL_RECORDS)
            .flat_map(|record| {
                record
                    .program_id()
                    .to_bytes_le()
                    .expect("Failed to convert program ID to bytes")
            })
            .collect::<Vec<_>>();

        let program_randomness = UniformRand::rand(rng);
        let program_commitment = C::program_commitment_scheme().commit(&program_ids, &program_randomness)?;
        Ok((program_commitment, program_randomness))
    }

    #[inline]
    pub fn to_encrypted_records<R: Rng + CryptoRng>(
        &self,
        rng: &mut R,
    ) -> Result<(
        Vec<EncryptedRecord<C>>,
        Vec<EncryptedRecordHash<C>>,
        Vec<EncryptedRecordRandomizer<C>>,
    )> {
        let mut encrypted_records = Vec::with_capacity(C::NUM_OUTPUT_RECORDS);
        let mut encrypted_record_hashes = Vec::with_capacity(C::NUM_OUTPUT_RECORDS);
        let mut encrypted_record_randomizers = Vec::with_capacity(C::NUM_OUTPUT_RECORDS);

        for record in self.output_records.iter().take(C::NUM_OUTPUT_RECORDS) {
            let (encrypted_record, encrypted_record_randomizer) = EncryptedRecord::encrypt(record, rng)?;
            encrypted_record_hashes.push(encrypted_record.to_hash()?);
            encrypted_records.push(encrypted_record);
            encrypted_record_randomizers.push(encrypted_record_randomizer);
        }

        Ok((encrypted_records, encrypted_record_hashes, encrypted_record_randomizers))
    }
}

impl<C: Parameters> ToBytes for TransactionAuthorization<C> {
    #[inline]
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        self.kernel.write_le(&mut writer)?;
        self.input_records.write_le(&mut writer)?;
        self.output_records.write_le(&mut writer)?;
        self.signatures.write_le(&mut writer)?;

        // Serialize the noop compute keys with Option ordering in order to
        // dedup with user-specified compute keys during execution.
        for noop_compute_key in self.noop_compute_keys.iter().take(C::NUM_INPUT_RECORDS) {
            match noop_compute_key {
                Some(noop_compute_key) => {
                    true.write_le(&mut writer)?;
                    noop_compute_key.write_le(&mut writer)?;
                }
                None => false.write_le(&mut writer)?,
            }
        }
        Ok(())
    }
}

impl<C: Parameters> FromBytes for TransactionAuthorization<C> {
    #[inline]
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        let kernel: TransactionKernel<C> = FromBytes::read_le(&mut reader)?;

        let mut input_records = Vec::<Record<C>>::with_capacity(C::NUM_INPUT_RECORDS);
        for _ in 0..C::NUM_INPUT_RECORDS {
            input_records.push(FromBytes::read_le(&mut reader)?);
        }

        let mut output_records = Vec::<Record<C>>::with_capacity(C::NUM_OUTPUT_RECORDS);
        for _ in 0..C::NUM_OUTPUT_RECORDS {
            output_records.push(FromBytes::read_le(&mut reader)?);
        }

        let mut signatures = Vec::<C::AccountSignature>::with_capacity(C::NUM_INPUT_RECORDS);
        for _ in 0..C::NUM_INPUT_RECORDS {
            signatures.push(FromBytes::read_le(&mut reader)?);
        }

        let mut noop_compute_keys = Vec::<Option<ComputeKey<C>>>::with_capacity(C::NUM_INPUT_RECORDS);
        for _ in 0..C::NUM_INPUT_RECORDS {
            let option_indicator: bool = FromBytes::read_le(&mut reader)?;
            match option_indicator {
                true => noop_compute_keys.push(Some(FromBytes::read_le(&mut reader)?)),
                false => noop_compute_keys.push(None),
            }
        }

        Ok(Self {
            kernel,
            input_records,
            output_records,
            signatures,
            noop_compute_keys,
        })
    }
}

impl<C: Parameters> FromStr for TransactionAuthorization<C> {
    type Err = DPCError;

    fn from_str(kernel: &str) -> Result<Self, Self::Err> {
        Ok(Self::read_le(&hex::decode(kernel)?[..])?)
    }
}

impl<C: Parameters> fmt::Display for TransactionAuthorization<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            hex::encode(to_bytes_le![self].expect("couldn't serialize to bytes"))
        )
    }
}