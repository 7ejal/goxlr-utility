//! Builds an emulator-only expanded ARM image. The updater magic is invalidated.
use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
};

fn word(image: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(image[offset..offset + 4].try_into().unwrap())
}

fn put(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn crc(mut state: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        state ^= u32::from(*byte);
        for _ in 0..8 {
            state = (state >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(state & 1));
        }
    }
    state
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 3
        || args[2]
            .extension()
            .is_none_or(|extension| extension != "gxlab")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Pass 907-image stage.bin output.gxlab",
        ));
    }
    let original = fs::read(&args[0])?;
    let stage = fs::read(&args[1])?;
    assert_eq!(original.len(), 640044);
    assert_eq!(&original[..16], b"GoXLR Firmware\0\0");
    assert_eq!(word(&original, 0x1c), 907);
    assert_eq!(word(&original, 0x28), 0xe897);
    assert_eq!(word(&original, 0x14), 0x6c63d1f0);
    assert_eq!(
        crc(u32::MAX, &original[0x18..]) ^ u32::MAX,
        word(&original, 0x14)
    );
    assert_eq!(crc(0, &original[0x54..0x3a2ac]), word(&original, 0x3a2ac));
    assert_eq!(&original[0x3a2b0..0x3a2ba], b"TCHDSP_V2:");
    assert!(!stage.is_empty() && stage.len() % 4 == 0);
    // Leave the active SVC stack and its saved registers outside the staging bytes.
    assert!(0x3a458 + stage.len() < 0x3a940);
    assert_eq!(word(&stage, 0), 0xe92d500f);
    let hook = 0x314 - 0x1ac;
    assert_eq!(word(&original, hook), 0xe59f137c);

    let mut lab = original[..0x3a2ac].to_vec();
    lab.extend_from_slice(&stage);
    lab.extend_from_slice(&[0; 4]);
    lab.extend_from_slice(&original[0x3a2b0..]);
    lab[..16].copy_from_slice(b"GXLAB_NOT_FLASH!");
    let length = lab.len() as u32 - 0x24;
    put(&mut lab, 0x20, length);
    put(&mut lab, 0x28, 0xe897 + stage.len() as u32 / 4);
    put(&mut lab, hook, 0xea000000 | ((0x3a458 - 0x314 - 8) / 4));
    let checksum_at = 0x3a2ac + stage.len();
    let arm_crc = crc(0, &lab[0x54..checksum_at]);
    put(&mut lab, checksum_at, arm_crc);
    let outer_crc = crc(u32::MAX, &lab[0x18..]) ^ u32::MAX;
    put(&mut lab, 0x14, outer_crc);
    assert_eq!(&lab[checksum_at + 4..], &original[0x3a2b0..]);

    // Every serialized edit is reversible to the exact supplied 907 image.
    let mut reversed = lab[..0x3a2ac].to_vec();
    reversed.extend_from_slice(&original[0x3a2ac..0x3a2b0]);
    reversed.extend_from_slice(&lab[checksum_at + 4..]);
    reversed[..16].copy_from_slice(&original[..16]);
    for offset in [0x14, 0x20, 0x28, hook] {
        put(&mut reversed, offset, word(&original, offset));
    }
    assert_eq!(reversed, original);

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    file.write_all(&lab)?;
    println!(
        "LAB ONLY {} bytes, ARM CRC {arm_crc:08X}, outer CRC {outer_crc:08X}",
        lab.len()
    );
    println!("Expanded ARM count and Boot2 contiguous loading remain a hypothesis.");
    Ok(())
}
