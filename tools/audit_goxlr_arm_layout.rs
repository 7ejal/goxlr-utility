//! Reads the 901/907 image layout and checks the ARM startup landmarks used by the EQ work.
use std::{env, fs, io};

const ARM_FILE_START: usize = 0x54;
const ARM_VA_START: usize = 0x200;
const VA_OFFSET: usize = ARM_VA_START - ARM_FILE_START;

fn u32_at(image: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(image[offset..offset + 4].try_into().unwrap())
}

fn arm_word(image: &[u8], address: usize) -> u32 {
    u32_at(image, address - VA_OFFSET)
}

fn check(condition: bool, reason: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, reason))
    }
}

fn main() -> io::Result<()> {
    let path = env::args_os().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Pass the full-size GoXLR firmware image",
        )
    })?;
    let image = fs::read(path)?;
    check(
        image.len() > 0x3a2c6 && image.starts_with(b"GoXLR Firmware\0\0"),
        "Unsupported GoXLR image",
    )?;
    check(
        u32_at(&image, 0x20) as usize + 0x24 == image.len(),
        "Outer length mismatch",
    )?;

    let arm_words = u32_at(&image, 0x28) as usize;
    let dsp_header = ARM_FILE_START + 4 * arm_words;
    check(
        dsp_header == 0x3a2b0 && &image[dsp_header..dsp_header + 10] == b"TCHDSP_V2:",
        "ARM section size does not match the known DSP header",
    )?;
    check(
        arm_word(&image, 0x314) == 0xe59f137c
            && arm_word(&image, 0x318) == 0xe59f237c
            && arm_word(&image, 0x328) == 0xe4810004,
        "Unknown ARM startup code",
    )?;
    let bss_start = arm_word(&image, 0x698) as usize;
    let bss_end = arm_word(&image, 0x69c) as usize;
    let arm_crc = dsp_header - 4;
    check(
        bss_start == arm_crc + VA_OFFSET && bss_end > bss_start,
        "Unexpected ARM/BSS boundary",
    )?;

    let constructors = arm_word(&image, 0xf178) as usize;
    let constructors_end = arm_word(&image, 0xf17c) as usize;
    check(
        constructors_end == bss_start
            && constructors < constructors_end
            && constructors >= ARM_VA_START
            && (constructors_end - constructors) % 4 == 0,
        "Unexpected constructor table",
    )?;

    println!(
        "ARM stored words {arm_words:#x}, checksum file offset {arm_crc:#x}, DSP header {dsp_header:#x}"
    );
    println!("ARM initialized VA {ARM_VA_START:#x}..{bss_start:#x}");
    println!("ARM constructor pointers {constructors:#x}..{constructors_end:#x}");
    println!("ARM zeroed at startup {bss_start:#x}..{bss_end:#x}");
    for (mode, literal) in [
        ("IRQ/undefined", 0x690),
        ("fault-handler", 0x694),
        ("early SVC", 0x6c8),
        ("application SVC", 0x68c),
    ] {
        let top = arm_word(&image, literal);
        check(
            (bss_start as u32..bss_end as u32).contains(&top),
            "Unexpected stack pointer",
        )?;
        println!("{mode} stack pointer {top:#x}");
    }

    for (name, address) in [
        ("effects", 0x38dd4),
        ("SHI transmit", 0x38bcc),
        ("SHI receive", 0x38bbc),
    ] {
        let buffer = arm_word(&image, address) as usize;
        let size = arm_word(&image, address + 4) as usize;
        check(
            buffer >= bss_start && size != 0 && buffer + size <= bss_end,
            "Unexpected ring backing storage",
        )?;
        println!(
            "{name} ring {address:#x}, backing storage {buffer:#x}..{:#x}",
            buffer + size
        );
    }
    check(
        arm_word(&image, 0x1280c) == 0xe59f10c4
            && arm_word(&image, 0x12818) == 0xe2612805
            && arm_word(&image, 0x128d8) as usize == bss_end,
        "Unexpected heap initialization",
    )?;
    println!(
        "Stock heap {bss_end:#x}..0x50000, {:#x} bytes",
        0x50000 - bss_end
    );
    println!("Appending code at {bss_start:#x} overlaps startup-cleared memory.");
    Ok(())
}
