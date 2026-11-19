//! Builds expanded lab images or a same-size experimental ARM image.
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

fn same_size_image(
    original: &[u8],
    bridge: &[u8],
    stage: &[u8],
    firmware: bool,
    build: u32,
) -> Vec<u8> {
    const BRIDGE: usize = 0x394d0;
    const STAGE: usize = 0x39d8c;
    const ENTRY: usize = 0x244 - 0x1ac;
    assert!(!bridge.is_empty() && bridge.len() <= 1000 && bridge.len() % 4 == 0);
    assert!(!stage.is_empty() && stage.len() <= 864 && stage.len() % 4 == 0);
    assert_eq!(word(stage, 0), 0xe92d503f);
    assert!(
        original[BRIDGE..BRIDGE + bridge.len()]
            .iter()
            .all(|&byte| byte == 0)
    );
    assert!(
        original[STAGE..STAGE + stage.len()]
            .iter()
            .all(|&byte| byte == 0)
    );
    assert_eq!(word(original, ENTRY), 0xeb003c51);
    let mut image = original.to_vec();
    image[BRIDGE..BRIDGE + bridge.len()].copy_from_slice(bridge);
    image[STAGE..STAGE + stage.len()].copy_from_slice(stage);
    put(&mut image, ENTRY, 0xea000000 | ((0x39f38 - 0x244 - 8) / 4));
    for offset in [0x1c, 0x2c, 0x3a2be] {
        put(&mut image, offset, build);
    }
    if !firmware {
        image[..16].copy_from_slice(b"GXLAB_NOT_FLASH!");
    }
    let arm_crc = crc(0, &image[0x54..0x3a2ac]);
    put(&mut image, 0x3a2ac, arm_crc);
    let outer_crc = crc(u32::MAX, &image[0x18..]) ^ u32::MAX;
    put(&mut image, 0x14, outer_crc);
    let mut reversed = image.clone();
    reversed[BRIDGE..BRIDGE + bridge.len()].fill(0);
    reversed[STAGE..STAGE + stage.len()].fill(0);
    reversed[..16].copy_from_slice(&original[..16]);
    for offset in [ENTRY, 0x14, 0x1c, 0x2c, 0x3a2be, 0x3a2ac] {
        put(&mut reversed, offset, word(original, offset));
    }
    assert_eq!(reversed, original);
    assert_eq!(word(&image, 0x28), word(original, 0x28));
    assert_eq!(image.len(), original.len());
    println!("Same-size experimental {build}, ARM CRC {arm_crc:08X}, outer CRC {outer_crc:08X}");
    image
}

fn main() -> io::Result<()> {
    let mut raw: Vec<_> = env::args_os().skip(1).collect();
    let build = if raw.first().is_some_and(|arg| arg == "--build") {
        assert!(raw.len() >= 2, "Pass a reserved build number");
        let build = raw[1].to_str().unwrap().parse::<u32>().unwrap();
        assert!([908, 909].contains(&build), "Build number is not reserved");
        raw.drain(..2);
        build
    } else {
        908
    };
    let args: Vec<_> = raw.into_iter().map(PathBuf::from).collect();
    assert!(
        build == 908 || args.len() == 4,
        "909 requires the same-size bridge form"
    );
    if ![3, 4].contains(&args.len())
        || args
            .last()
            .and_then(|path| path.extension())
            .is_none_or(|ext| ext != "gxlab" && !(args.len() == 4 && ext == "bin"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Pass 907-image stage.bin output.gxlab, or 907-image bridge.bin same-size-stage.bin output.gxlab|bin",
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
    if args.len() == 4 {
        let image = same_size_image(
            &original,
            &stage,
            &fs::read(&args[2])?,
            args[3].extension().is_some_and(|ext| ext == "bin"),
            build,
        );
        return fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[3])?
            .write_all(&image);
    }
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
