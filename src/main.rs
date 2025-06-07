use password_display::*;
mod qr_code;
use std::fs;

fn main() {
    // This program reads a password from a file and displays it as a QR code.
    // The maximum length allowed is 256 bits.

    // Check for the right number of arguments
    // Give a warning if there are too many or not enough
    let path = match read_args() {
        Ok(path) => path,
        Err(err) => {
            println!("{err}");
            return;
        }
    };

    // Read bits from file (assumes that all passwords are full bytes)
    // Store the length of password for later use
    let file_length = fs::metadata(&path).unwrap().len();
    let password_length_bytes: u8;

    if file_length > 32 {
        println!(
            "File length: {} bits\nOnly first 256 bits will be processed",
            file_length * 8
        );
        password_length_bytes = 32;
    } else {
        password_length_bytes = file_length as u8;
    }

    let bits: Vec<u8> = match read_bits(path, &password_length_bytes) {
        Ok(vec) => vec,
        Err(err) => {
            println!("{err}");
            return;
        }
    };

    // Transform raw bits into a fully formed QR code

    // Encode the binary stream in base45 / alphanumeric
    let encoded_bits = qr_code::encode_bits(bits, 45);

    // Add mode indicator, length indicator, padding, etc.
    let data_bits = qr_code::encapsulate_data(encoded_bits);

    // Add 10 error correction codewords
    let data_ecc = qr_code::apply_ecc(data_bits);

    // Start with an empty 25x25 matrix
    let mut matrix = qr_code::Matrix::new();

    // Populate it with fixed patterns, data, and format information
    matrix.place_finder_pattern();
    matrix.place_alignment_pattern();
    matrix.place_dark_module();
    matrix.place_timing_pattern();
    matrix.reserve_format_area();
    matrix.fill_data(data_ecc);
    matrix.mask_and_place_format_string();

    // Save the final matrix of black and white modules and add four
    // modules of white space on all sides
    let qr_final = matrix.export();

    // Form a PNG and write it to disk
    let png = form_png(qr_final);

    fs::write("./qr_code.png", png).expect("Unable to write file");
}
