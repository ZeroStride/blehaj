use std::{cmp, fmt::Debug};

use serde::{Deserialize, Serialize};
use tracing::{instrument, trace};

const CHONK_FLAG_SOF: u8 = 0b000000001;
const CHONK_FLAG_EOF: u8 = 0b000000010;

/// Oh lawd.
#[derive(Deserialize, Serialize)]
pub struct BleChonk {
    pub flags: u8,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

impl Debug for BleChonk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("BleChonk")
            .field("is_start_of_frame", &self.is_start_of_frame())
            .field("is_end_of_frame", &self.is_start_of_frame())
            .field("data.len", &self.data.len())
            .finish()
    }
}

impl TryInto<Vec<u8>> for BleChonk {
    type Error = rmp_serde::encode::Error;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        rmp_serde::to_vec(&self)
    }
}

impl TryFrom<Vec<u8>> for BleChonk {
    type Error = rmp_serde::decode::Error;
    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        rmp_serde::from_slice(&bytes)
    }
}

impl BleChonk {
    pub fn is_start_of_frame(&self) -> bool {
        (self.flags & CHONK_FLAG_SOF) > 0
    }

    pub fn is_end_of_frame(&self) -> bool {
        (self.flags & CHONK_FLAG_EOF) > 0
    }
}

#[derive(Debug)]
pub struct Chonker {
    pub mtu: usize,
    pub chonk_size: usize,
}

impl Chonker {
    pub fn new(mtu: usize) -> Self {
        let msgpack_headers: usize = 1 + // flags header
                                     5; //  data header (maximum size)
        Chonker {
            mtu,
            chonk_size: mtu - msgpack_headers - 1, // one byte for flags
        }
    }

    #[instrument(skip(data))]
    pub fn chonk_data(&self, data: &[u8]) -> Vec<BleChonk> {
        let num_chonks = (data.len() / self.chonk_size)
            + if data.len().is_multiple_of(self.chonk_size) {
                0
            } else {
                1
            };
        trace!(num_chonks, "chonking data");

        let mut chonks: Vec<BleChonk> = Vec::new();
        for i in (0..num_chonks).rev() {
            let mut flags: u8 = 0;
            if i == 0 {
                flags |= CHONK_FLAG_SOF
            }
            if i == (num_chonks - 1) {
                flags |= CHONK_FLAG_EOF
            }
            let offset = i * self.chonk_size;
            let len = cmp::min(data.len() - offset, self.chonk_size);
            trace!(chonk = i, len, flags, "pushing chonk");
            let data = data[offset..(offset + len)].to_vec();
            assert!(
                data.len() < self.mtu,
                "You messed up the msgpack math, Ezri."
            );
            chonks.push(BleChonk { flags, data })
        }
        chonks
    }
}
