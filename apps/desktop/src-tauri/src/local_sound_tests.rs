use super::*;
use tempfile::tempdir;

pub(crate) fn wav() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&40_u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&44100_u32.to_le_bytes());
    bytes.extend_from_slice(&88200_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes
}

#[test]
fn wav_validation_rejects_fake_truncated_compressed_and_empty_audio() {
    let root = tempdir().unwrap();
    let path = root.path().join("tone.WAV");
    fs::write(&path, wav()).unwrap();
    assert!(sound_bytes(&path).is_ok());
    for invalid in [
        b"not WAV".to_vec(),
        wav()[..40].to_vec(),
        {
            let mut bytes = wav();
            bytes[20] = 2;
            bytes
        },
        {
            let mut bytes = wav();
            bytes[40..44].copy_from_slice(&0_u32.to_le_bytes());
            bytes
        },
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(sound_bytes(&path).is_err());
    }
}
