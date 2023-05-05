use enum_iterator::{
    all,
    Sequence,
};
use fuel_core_types::{
    blockchain::header::{
        ApplicationHeader,
        ConsensusHeader,
        PartialBlockHeader,
    },
    fuel_crypto::generate_mnemonic_phrase,
    fuel_tx::UtxoId,
    fuel_types::{
        Address,
        AssetId,
        Bytes32,
        Word, Nonce,
    },
    fuel_vm::SecretKey,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

mod in_out;

pub struct Data {
    address: Box<dyn Iterator<Item = Address>>,
    asset_id: Box<dyn Iterator<Item = AssetId>>,
    word: Box<dyn Iterator<Item = Word>>,
    secret_key: Box<dyn Iterator<Item = SecretKey>>,
    utxo_id: Box<dyn Iterator<Item = UtxoId>>,
    nonce: Box<dyn Iterator<Item = Nonce>>,
}

impl Data {
    pub fn new() -> Self {
        let address = Box::new(all::<[u8; 32]>().cycle().map(Address::from));
        let asset_id = Box::new(all::<[u8; 32]>().cycle().map(AssetId::from));
        let word = Box::new(all::<u16>().cycle().map(Word::from));
        let secret_key = Box::new(all::<[u8; 32]>().cycle().map(secret_key));
        let utxo_id = Box::new(
            all::<[u8; 32]>()
                .cycle()
                .zip(all::<u8>().cycle())
                .map(|(a, o)| UtxoId::new(Bytes32::from(a), o)),
        );
        let nonce = Box::new(all::<[u8; 32]>().cycle().map(Nonce::from));
        Self {
            address,
            asset_id,
            word,
            secret_key,
            utxo_id,
            nonce,
        }
    }

    pub fn address(&mut self) -> Address {
        self.address.next().unwrap()
    }

    pub fn asset_id(&mut self) -> AssetId {
        self.asset_id.next().unwrap()
    }

    pub fn word(&mut self) -> Word {
        self.word.next().unwrap()
    }

    pub fn secret_key(&mut self) -> SecretKey {
        self.secret_key.next().unwrap()
    }

    pub fn utxo_id(&mut self) -> UtxoId {
        self.utxo_id.next().unwrap()
    }

    pub fn nonce(&mut self) -> Nonce {
        self.nonce.next().unwrap()
    }
}

impl Default for Data {
    fn default() -> Self {
        Self::new()
    }
}

fn secret_key(seed: [u8; 32]) -> SecretKey {
    let phrase = generate_mnemonic_phrase(&mut StdRng::from_seed(seed), 24).unwrap();
    SecretKey::new_from_mnemonic_phrase_with_path(&phrase, "m/44'/60'/0'/0/0").unwrap()
}

pub fn make_header() -> PartialBlockHeader {
    PartialBlockHeader {
        application: ApplicationHeader {
            da_height: 1u64.into(),
            generated: Default::default(),
        },
        consensus: ConsensusHeader {
            prev_root: Bytes32::zeroed(),
            height: 1u32.into(),
            time: fuel_core_types::tai64::Tai64::now(),
            generated: Default::default(),
        },
    }
}
