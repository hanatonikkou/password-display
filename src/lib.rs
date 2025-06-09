use std::env;
use std::fs;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;

// Read arguments from command line
// Check for exactly one
// Transform that one into canonical filepath
pub fn read_args() -> Result<PathBuf, &'static str> {
    if env::args().nth(2).is_some() {
        return Err("Too many arguments");
    }

    let filepath_from_args = match env::args().nth(1) {
        Some(string) => string,
        None => return Err("Please provide a password file"),
    };

    match fs::canonicalize(filepath_from_args) {
        Ok(path) => {
            return Ok(path);
        }
        Err(_) => return Err("No such file or directory"),
    }
}

// Read a given number of bytes from a given filepath
pub fn read_bits(path: PathBuf, length: &u8) -> io::Result<Vec<u8>> {
    let f = BufReader::new(File::open(path)?);
    let mut bits: Vec<u8> = vec![];
    let mut i = 0;

    for byte in f.bytes() {
        bits.push(byte?);
        i += 1;
        if i == *length {
            break;
        }
    }

    return Ok(bits);
}

// Transform a QR matrix into PNG file
pub fn form_png(qr_matrix: [[u8; 33]; 33]) -> Vec<u8> {
    // Prepare the data:
    // Write the array of rows into one long stream of bits and insert a filter
    // type byte before every row.
    // Turn each QR module into an 8x8 square of pixels: Invert the color
    // representation (QR black: "1" to PNG black: "0"), inflate each module
    // to 8 pixels in a row, and copy each row seven times.
    let mut temp: Vec<u8>;
    let mut image_serial: Vec<u8> = vec![];

    for row in qr_matrix {
        // Invert color, expand pixels
        temp = row.iter().map(|x| if *x == 1 { 0 } else { 255 }).collect();
        for _n in 0..8 {
            image_serial.push(0); // Filter type: none
            image_serial.append(&mut temp.clone()); // Copy rows
        }
    }

    // Form a valid PNG file
    // Composed of four chunks: Signature, IHDR, IDAT, IEND
    let mut append: Vec<u8>;
    let mut png_image: Vec<u8> = vec![];

    append = png_signature();
    png_image.append(&mut append);

    append = png_ihdr();
    png_image.append(&mut append);

    append = png_idat(image_serial);
    png_image.append(&mut append);

    append = png_iend();
    png_image.append(&mut append);

    png_image
}

// Fixed start of every PNG file
fn png_signature() -> Vec<u8> {
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
}

// IHDR chunk
fn png_ihdr() -> Vec<u8> {
    let crc: u32;
    let mut ihdr: Vec<u8> = vec![
        0, 0, 0, 0x0D, // Length of data
        0x49, 0x48, 0x44, 0x52, // "IHDR"
        0, 0, 1, 8, // Image width: 264px
        0, 0, 1, 8, // Image height: 264px
        1, // Bit depth
        0, // Color type
        0, // Compression
        0, // Filter
        0, // Enlacement
    ];

    // CRC calculation excludes length field
    crc = calculate_crc(&ihdr[4..]);
    ihdr.push((crc >> 24) as u8);
    ihdr.push(((crc & 0x00FF_0000) >> 16) as u8);
    ihdr.push(((crc & 0x0000_FF00) >> 08) as u8);
    ihdr.push((crc & 0x0000_00FF) as u8);

    ihdr
}

// IDAT chunk contains the image itself
fn png_idat(image: Vec<u8>) -> Vec<u8> {
    // Deflate algorithm is mandatory
    let mut image_compressed = deflate(image);
    let crc: u32;
    let length: u32;
    // Preform IDAT chunk start
    let mut idat: Vec<u8> = vec![
        0, 0, 0, 0, // Length of data
        0x49, 0x44, 0x41, 0x54, // "IDAT"
    ];

    // Insert length of data field
    // (Exclude length field, chunk type, and CRC)
    length = image_compressed.len() as u32;
    idat[0] = (length >> 24) as u8;
    idat[1] = ((length & 0x00FF_0000) >> 16) as u8;
    idat[2] = ((length & 0x0000_FF00) >> 08) as u8;
    idat[3] = (length & 0x0000_00FF) as u8;

    idat.append(&mut image_compressed);

    // CRC calculation excludes length field
    crc = calculate_crc(&idat[4..]);
    idat.push((crc >> 24) as u8);
    idat.push(((crc & 0x00FF_0000) >> 16) as u8);
    idat.push(((crc & 0x0000_FF00) >> 08) as u8);
    idat.push((crc & 0x0000_00FF) as u8);

    idat
}

// IEND chunk
fn png_iend() -> Vec<u8> {
    vec![
        0, 0, 0, 0, // length of data: none
        0x49, 0x45, 0x4E, 0x44, // "IEND"
        0xAE, 0x42, 0x60, 0x82, // CRC hardcoded
    ]
}

// Deflate the image data
// Allows for uncompressed data, to avoid inflating already compressed
// data. Used here for simplicity.
fn deflate(mut data: Vec<u8>) -> Vec<u8> {
    let length: u16 = data.len() as u16; // Length of uncompressed data
    let adler32 = calculate_adler32(&data); // Compute checksum
    let mut deflate_block: Vec<u8> = vec![
        0x78,                         // Deflate header: Compression method
        0x01,                         // Deflate header: No compr., checksum
        0x01,                         // Block header: No compression, last block
        (length & 255) as u8,         // Length in two bytes
        (length >> 8) as u8,          // Little-endian order
        ((length & 255) as u8) ^ 255, // Length's one's complement
        ((length >> 8) as u8) ^ 255,  // Also little-endian
    ];

    deflate_block.append(&mut data); // Append unaltered data

    // Append Adler32 checksum in big-endian order
    deflate_block.push((adler32 >> 24) as u8);
    deflate_block.push(((adler32 & 0x00FF_0000) >> 16) as u8);
    deflate_block.push(((adler32 & 0x0000_FF00) >> 08) as u8);
    deflate_block.push((adler32 & 0x0000_00FF) as u8);

    deflate_block
}

// Calculate deflate checksum
fn calculate_adler32(data: &Vec<u8>) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;

    // S1 keeps a running sum of all the data bytes
    // S2 sums S1 in each round
    for byte in data {
        s1 = (s1 + *byte as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }

    s2 << 16 ^ s1 // Concatenate S1 and S2 for final checksum
}

// Calculate CRC32 for PNG chunks
fn calculate_crc(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    // Generator polynom as specified
    // Leading 1 omitted
    let gen_poly: u32 = 0b00000100110000010001110110110111;

    // Pre-populate CRC
    // Computation starts from LSB -> reflect bytes
    crc = crc ^ reflect_byte(data[0]) as u32;
    crc = crc << 8;
    crc = crc ^ reflect_byte(data[1]) as u32;
    crc = crc << 8;
    crc = crc ^ reflect_byte(data[2]) as u32;
    crc = crc << 8;
    crc = crc ^ reflect_byte(data[3]) as u32;

    // Specs say to initialize CRC with all 1
    // Effect is to invert first 32 bits
    crc = !crc;

    // Iterate for 4 * 8 loops after data is empty to generate 32 CRC bits
    for n in 4..data.len() + 4 {
        if n < data.len() {
            let byte = data[n];
            for m in 0..8 {
                // If the bit-about-to-be-discarded is 1, divide
                if crc & 0x80000000 == 0x80000000 {
                    crc = crc << 1;
                    crc += next_bit(byte, m);
                    crc = crc ^ gen_poly;

                // If not, simply shift
                } else {
                    crc = crc << 1;
                    crc += next_bit(byte, m);
                }
            }
        } else {
            // For the last iteration, don't add a new bit
            for _m in 0..8 {
                if crc & 0x80000000 == 0x80000000 {
                    crc = crc << 1;
                    crc = crc ^ gen_poly;
                } else {
                    crc = crc << 1;
                }
            }
        }
    }

    // Mirror and invert as per specs
    crc = mirror_crc(crc);
    crc = !crc;

    crc as u32
}

// Invert order of bits in a byte
fn reflect_byte(byte: u8) -> u8 {
    let mut new_byte: u8 = 0;
    for n in 0..8u32 {
        if byte & 2u8.pow(7 - n) == 2u8.pow(7 - n) {
            new_byte += 2u8.pow(n);
        }
    }
    new_byte
}

// Return bit at position n
fn next_bit(byte: u8, n: u32) -> u32 {
    if byte & 2u8.pow(n) == 2u8.pow(n) {
        1
    } else {
        0
    }
}

// Invert order of bits in a CRC32
fn mirror_crc(crc: u32) -> u32 {
    let mut new_crc: u32 = 0;
    for n in 0..32u32 {
        if crc & 2u32.pow(n) == 2u32.pow(n) {
            new_crc += 2u32.pow(31 - n);
        }
    }

    new_crc
}
