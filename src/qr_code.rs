// Represents a QR code, version 2: 25 x 25 modules
// Matrix::mask defines areas where data cannot be written
#[derive(Clone, Copy)]
pub struct Matrix {
    data: [[u8; 25]; 25],
    mask: [[bool; 25]; 25],
}

// Used as Point(row, column) in Matrix
struct Point(usize, usize);

// Encodes a binary stream in alphanumeric representation by treating the
// input as a single large number and repeatedly dividing it mod 45, saving
// the remainder as the new representation.
pub fn encode_bits(mut bits: Vec<u8>, base: u8) -> Vec<u8> {
    let mut encoded_bits: Vec<u8> = vec![];
    let input_length = bits.len();

    // divmod until the input is empty
    while bits.is_empty() == false {
        let divmod = divmod(bits, base);
        encoded_bits.insert(0, divmod.1); // populate encoded vector from LSB
        bits = divmod.0;
    }

    // If the bitstream starts with bytes of zero, the encoded vector may be
    // too short.
    // Expected length: Input_length * 8 bits/byte * 2 alphanumeric
    // characters / 11 input bits
    // Pad the result if necessary
    let temp = input_length * 8 * 2;
    let expected_length = temp.div_ceil(11);

    while encoded_bits.len() < expected_length {
        encoded_bits.insert(0, 0);
    }

    encoded_bits
}

// Divide a number of arbitraty length modulo any base, return both quotient
// and remainder
fn divmod(number: Vec<u8>, base: u8) -> (Vec<u8>, u8) {
    let mut temp: u16 = 0;
    let mut quotient: u16;
    let mut result: Vec<u8> = Vec::with_capacity(32);

    // Divide byte by byte
    for byte in number {
        temp = temp << 8; // left-shift remainder
        temp += byte as u16; // add next byte
        quotient = temp / base as u16; // calculate quotient
        temp = temp % base as u16; // calculate remainder
        if quotient == 0 && result.is_empty() {
            continue;
        } // remove leading empty bytes but keep ones in the middle

        result.push(quotient as u8); // build overall quotient byte by byte
    }

    // return quotient, remainder
    (result, temp as u8)
}

// Take message data and add everything needed to build a QR code
pub fn encapsulate_data(mut encoded_bits: Vec<u8>) -> [u8; 34 * 8] {
    // Version 2 (25x25), error correction level L:
    // 272 bits (34 bytes)
    let mut data = [0; 34 * 8];

    // Add mode indicator
    // 0010 = alphanumeric mode
    data[2] = 1;

    // Add length indicator
    // Count of alphanumeric characters, written into 9 bits
    let mut character_count = encoded_bits.len() as u8;
    let mut insert_bit: u8;
    let no_of_bits = 9;

    for n in 0..no_of_bits {
        insert_bit = character_count & 1;
        character_count = character_count >> 1;
        data[3 + no_of_bits - n] = insert_bit;
    }

    // Add message characters
    let mut index = 4 + no_of_bits;
    let mut temp: u16;
    let mut temp_vec: Vec<u8>;

    while encoded_bits.len() > 1 {
        // Collect characters in pairs
        temp_vec = encoded_bits.drain(0..2).collect();
        // Convert them to binary
        temp = temp_vec[0] as u16 * 45 + temp_vec[1] as u16;

        // Add them in 11-bit groups
        for _n in 0..11 {
            if temp & 1024 == 1024 {
                data[index] = 1
            } else {
                data[index] = 0
            }
            temp = temp << 1;
            index += 1;
        }
    }

    // If a single character's left over, add it as a 6-bit group
    if !encoded_bits.is_empty() {
        for _n in 0..6 {
            if encoded_bits[0] & 32 == 32 {
                data[index] = 1
            } else {
                data[index] = 0
            }
            encoded_bits[0] = encoded_bits[0] << 1;
            index += 1;
        }
    }

    // If there's space left over, add terminator of 0s
    // (maximum of four)
    for _n in 0..4 {
        if index == data.len() {
            break;
        }
        index += 1;
    }

    // Pad to full bytes
    while index % 8 != 0 {
        index += 1;
    }

    // Fill remaining space with padding bytes
    let mut temp: u8;

    loop {
        if index == data.len() {
            break;
        }

        temp = 236; // First pad byte

        for _n in 0..8 {
            if temp & 128 == 128 {
                data[index] = 1;
            } else {
                data[index] = 0;
            }
            temp = temp << 1;
            index += 1;
        }

        if index == data.len() {
            break;
        }

        temp = 17; // Second pad byte

        for _n in 0..8 {
            if temp & 128 == 128 {
                data[index] = 1;
            } else {
                data[index] = 0;
            }
            temp = temp << 1;
        }
    }

    data
}

// Calculate Reed-Solomon code words
pub fn apply_ecc(data: [u8; 34 * 8]) -> [u8; 44 * 8] {
    // Version 2, ECC level L needs 10 EC code words
    let mut index = 0;
    let mut message: [u8; 44] = [0; 44];
    let mut message_ecc: [u8; 44 * 8] = [0; 44 * 8];

    // Concatenate input bits into 8-bit message codewords
    for n in 0..34 {
        message[n] = data[index] * 128
            + data[index + 1] * 64
            + data[index + 2] * 32
            + data[index + 3] * 16
            + data[index + 4] * 8
            + data[index + 5] * 4
            + data[index + 6] * 2
            + data[index + 7];
        index += 8;
    }

    // Set initial state
    let mut temp: [u8; 11];
    let mut remainder: [u8; 11] = [0; 11];
    remainder.copy_from_slice(&message[..11]);

    // Generator polynom for GF(256), chosen by QR specs
    let gen_poly: [u8; 11] = [1, 216, 194, 159, 111, 199, 94, 95, 113, 157, 193];

    // Divide message by generator polynom using finite field arithmetic
    // Discard the quotient, keep the remainder
    // Remainder of the last division is the Reed-Solomon code
    for n in 0..34 {
        // Multiply the generator polynom with the first coefficient of
        // the current remainder
        temp = gf_multiply(gen_poly, remainder[0]);
        // Subtract (i.e. XOR) the result from the remainder, discard
        // the first element (which is 0), and shift everything to the
        // next position
        for m in 0..10 {
            remainder[m] = remainder[m + 1] ^ temp[m + 1];
        }
        // If this is not the last round, fill in the last place of the remainder
        // with a new byte from the message to be divided
        if n != 33 {
            remainder[10] = message[n + 11];
        }
    }

    // Copy input data data to message_ecc, which has space for the
    // error correction codes
    message_ecc[..34 * 8].copy_from_slice(&data);
    index = 34 * 8;

    // Convert error correction codewords into bits and append each one
    // Discard remainder[10], which is only used for computing
    for n in 0..10 {
        if remainder[n] & 128 == 128 {
            message_ecc[index + 0] = 1;
        }
        if remainder[n] & 64 == 64 {
            message_ecc[index + 1] = 1;
        }
        if remainder[n] & 32 == 32 {
            message_ecc[index + 2] = 1;
        }
        if remainder[n] & 16 == 16 {
            message_ecc[index + 3] = 1;
        }
        if remainder[n] & 8 == 8 {
            message_ecc[index + 4] = 1;
        }
        if remainder[n] & 4 == 4 {
            message_ecc[index + 5] = 1;
        }
        if remainder[n] & 2 == 2 {
            message_ecc[index + 6] = 1;
        }
        if remainder[n] & 1 == 1 {
            message_ecc[index + 7] = 1;
        }
        index += 8;
    }

    message_ecc
}

// Multiplication function using finite field arithmetic
fn gf_multiply(gen_poly: [u8; 11], factor: u8) -> [u8; 11] {
    let mut temp: u16;
    let mut mask: u16;
    let mut factor_1: u16;
    let mut result: [u8; 11] = [0; 11];

    let factor_2 = factor as u16;

    // Multiply each coefficient in turn
    for n in 0..11 {
        temp = 0;
        mask = 1;
        factor_1 = gen_poly[n] as u16;

        // Multiply the factors, adding without carry (bitwise mod 2)
        while mask < 255 {
            temp = ((factor_2 & mask) * factor_1) ^ temp;
            mask = mask << 1;
        }

        // Substitute bits > 255 according to log-antilog table and add together
        if temp & mask == mask {
            temp = temp ^ 29;
        }
        mask = mask << 1;
        if temp & mask == mask {
            temp = temp ^ 58;
        }
        mask = mask << 1;
        if temp & mask == mask {
            temp = temp ^ 116;
        }
        mask = mask << 1;
        if temp & mask == mask {
            temp = temp ^ 232;
        }
        mask = mask << 1;
        if temp & mask == mask {
            temp = temp ^ 205;
        }
        mask = mask << 1;
        if temp & mask == mask {
            temp = temp ^ 135;
        }
        mask = mask << 1;
        if temp & mask == mask {
            temp = temp ^ 19;
        }
        mask = mask << 1;
        if temp & mask == mask {
            temp = temp ^ 38;
        }

        // Remove bits > 255
        temp = temp & 255;

        result[n] = temp as u8;
    }

    result
}

// Representation of a 2D QR code and methods for preparing, populating, and extracting it
impl Matrix {
    // Every module (black or white square) in the final QR code is represented
    // by one u8: 0 - white, 1 - black
    // Keep track of prohibited areas, where data can't be written
    pub fn new() -> Matrix {
        Matrix {
            data: [[0; 25]; 25],
            mask: [[false; 25]; 25],
        }
    }

    pub fn place_finder_pattern(&mut self) {
        let point_1 = Point(0, 0);
        let point_2 = Point(18, 0);
        let point_3 = Point(0, 18);
        let points = [&point_1, &point_2, &point_3];

        for point in points {
            // Apply pattern
            for n in 0..7 {
                self.data[point.0 + 0][point.1 + n] = 1;
            }
            for n in 0..7 {
                self.data[point.0 + n][point.1 + 0] = 1;
            }
            for n in 0..7 {
                self.data[point.0 + 6][point.1 + n] = 1;
            }
            for n in 0..7 {
                self.data[point.0 + n][point.1 + 6] = 1;
            }
            for n in 0..3 {
                for i in 0..3 {
                    self.data[point.0 + 2 + n][point.1 + 2 + i] = 1;
                }
            }
        }

        // Block area for data bits
        // Include a separating line next to the finder patterns
        for n in 0..8 {
            for i in 0..8 {
                self.mask[point_1.0 + n][point_1.1 + i] = true;
            }
        }
        for n in 0..8 {
            for i in 0..8 {
                self.mask[point_2.0 - 1 + n][point_2.1 + i] = true;
            }
        }
        for n in 0..8 {
            for i in 0..8 {
                self.mask[point_3.0 + n][point_3.1 - 1 + i] = true;
            }
        }
    }

    pub fn place_alignment_pattern(&mut self) {
        let point = Point(18, 18);

        // Place pattern
        self.data[point.0][point.1] = 1;
        for n in 0..5 {
            self.data[point.0 - 2 + 0][point.1 - 2 + n] = 1;
        }
        for n in 0..5 {
            self.data[point.0 - 2 + n][point.1 - 2 + 0] = 1;
        }
        for n in 0..5 {
            self.data[point.0 + 2 + 0][point.1 - 2 + n] = 1;
        }
        for n in 0..5 {
            self.data[point.0 - 2 + n][point.1 + 2 + 0] = 1;
        }

        // Block area for data bits
        for n in 0..5 {
            for i in 0..5 {
                self.mask[point.0 - 2 + n][point.1 - 2 + i] = true;
            }
        }
    }

    // There's always one black module next to the lower left finder pattern
    pub fn place_dark_module(&mut self) {
        let point = Point(17, 8);
        self.data[point.0][point.1] = 1;
        self.mask[point.0][point.1] = true;
    }

    // One row and one column of alternating black and white modules
    pub fn place_timing_pattern(&mut self) {
        for n in 0..9 {
            if n % 2 == 0 {
                self.data[6][8 + n] = 1;
            }
            self.mask[6][8 + n] = true;
        }

        for n in 0..9 {
            if n % 2 == 0 {
                self.data[8 + n][6] = 1;
            }
            self.mask[8 + n][6] = true;
        }
    }

    // Reserve space for formatting information
    // Includes which masking pattern was used, which will be determined later
    // Will be added at the last step
    pub fn reserve_format_area(&mut self) {
        for n in 0..25 {
            if n <= 8 || n >= 17 {
                self.mask[8][n] = true;
                self.mask[n][8] = true;
            }
        }
    }

    // After all the fixed modules have been placed, fill remainder with data
    // As calculated, the data array has 7 bits fewer than there are empty
    // modules. Per QR specs, these should be filled with 0s. Since the array
    // was initialized as all 0s, they don't have to be explicitly added to
    // the input data.
    pub fn fill_data(&mut self, data_bits: [u8; 44 * 8]) {
        // Set initial state
        // Start at lower right corner of the matrix and at bit 0 of data
        let mut col = 24;
        let mut row = 24;
        let mut index = 0;

        // Place bits one by one into modules
        while index < data_bits.len() {
            // Go upward, alternately filling two columns
            // Only fill a module if it isn't masked
            loop {
                if !self.mask[row][col] {
                    self.data[row][col] = data_bits[index];
                    index += 1;
                }
                if index == data_bits.len() {
                    break;
                }
                if !self.mask[row][col - 1] {
                    self.data[row][col - 1] = data_bits[index];
                    index += 1;
                }
                if index == data_bits.len() {
                    break;
                }
                if row == 0 {
                    break;
                }
                row -= 1;
            }

            // Move over two columns
            // Skip the vertical timing pattern entirely
            col -= 2;
            if col == 6 {
                col -= 1;
            }

            // Go downward, alternately filling two columns
            // Only fill a module if it isn't masked
            loop {
                if !self.mask[row][col] {
                    self.data[row][col] = data_bits[index];
                    index += 1;
                }
                if index == data_bits.len() {
                    break;
                }
                if !self.mask[row][col - 1] {
                    self.data[row][col - 1] = data_bits[index];
                    index += 1;
                }
                if index == data_bits.len() {
                    break;
                }
                if row == 24 {
                    break;
                }
                row += 1;
            }

            if col > 1 {
                col -= 2;
            }
            if col == 6 {
                col -= 1;
            }
        }
    }

    // Masks flip certain modules to reduce areas which are difficult
    // to scan correctly.
    // 8 masking patterns exist. Apply each one in turn, evaluate all of
    // them, then choose the one with the lowest penalty score.
    // Use the chosen pattern for the matrix.
    pub fn mask_and_place_format_string(&mut self) {
        let mut all_masks = [self.clone(); 8];
        let mut lowest_score = (0, usize::MAX);

        for n in 0..8 {
            all_masks[n].transform(n);
            if all_masks[n].evaluate() < lowest_score.1 {
                lowest_score.0 = n;
                lowest_score.1 = all_masks[n].evaluate();
            }
        }

        self.data = all_masks[lowest_score.0].data;
        self.place_format_string(lowest_score.0);
    }

    // Toggle bits in a matrix following a predefined pattern
    fn transform(&mut self, mask_no: usize) {
        // Choose masking pattern
        let eval = match mask_no {
            0 => |row, col| (row + col) % 2,
            1 => |row, _col| row % 2,
            2 => |_row, col| col % 3,
            3 => |row, col| (row + col) % 3,
            4 => |row: usize, col: usize| (row.div_euclid(2) + col.div_euclid(3)) % 2,
            5 => |row, col| (row * col) % 2 + (row * col) % 3,
            6 => |row, col| ((row * col) % 2 + (row * col) % 3) % 2,
            _ => |row, col| ((row + col) % 2 + (row * col) % 3) % 2,
        };

        // Apply masking pattern
        // Only toggle data bits
        for row in 0..25 {
            for col in 0..25 {
                if self.mask[row][col] {
                    continue;
                }

                if eval(row, col) == 0 {
                    self.flip_bit(row, col);
                }
            }
        }
    }

    fn flip_bit(&mut self, row: usize, col: usize) {
        if self.mask[row][col] == true {
            return;
        }

        if self.data[row][col] == 1 {
            self.data[row][col] = 0;
        } else {
            self.data[row][col] = 1;
        }
    }

    // Search for certain patterns and tally a penalty score
    fn evaluate(&self) -> usize {
        let mut score: usize = 0;

        // Rule 1: Five or more same-colored modules
        let mut pattern = [0; 5];
        let mut continuous = false;

        // Rule 1 in rows
        for row in 0..25 {
            for col in 0..21 {
                for n in 0..5 {
                    pattern[n] = self.data[row][col + n];
                }

                if pattern == [0; 5] || pattern == [1; 5] {
                    if !continuous {
                        score += 3;
                        continuous = true;
                    } else {
                        score += 1
                    }
                } else {
                    continuous = false;
                }
            }
        }

        // Rule 1 in columns
        continuous = false;
        for row in 0..21 {
            for col in 0..25 {
                for n in 0..5 {
                    pattern[n] = self.data[row + n][col];
                }

                if pattern == [0; 5] || pattern == [1; 5] {
                    if !continuous {
                        score += 3;
                        continuous = true;
                    } else {
                        score += 1
                    }
                } else {
                    continuous = false;
                }
            }
        }

        // Rule 2: same-coloured modules in a 2x2 square
        let mut pattern = [0; 4];

        for row in 0..23 {
            for col in 0..23 {
                pattern[0] = self.data[row + 0][col + 0];
                pattern[1] = self.data[row + 1][col + 0];
                pattern[2] = self.data[row + 0][col + 1];
                pattern[3] = self.data[row + 1][col + 1];

                if pattern == [0; 4] || pattern == [1; 4] {
                    score += 3
                }
            }
        }

        // Rule 3: Patterns similar to finder patterns
        let mut pattern = [0; 11];
        let search_ptn_1 = [0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1];
        let search_ptn_2 = [1, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0];

        // Rule 3 in rows
        for row in 0..25 {
            for col in 0..15 {
                for n in 0..11 {
                    pattern[n] = self.data[row][col + n];
                }

                if pattern == search_ptn_1 || pattern == search_ptn_2 {
                    score += 40;
                }
            }
        }

        // Rule 3 in columns
        for row in 0..15 {
            for col in 0..25 {
                for n in 0..11 {
                    pattern[n] = self.data[row + n][col];
                }

                if pattern == search_ptn_1 || pattern == search_ptn_2 {
                    score += 40;
                }
            }
        }

        // Rule 4: Ratio of dark to light modules
        let mut count_dark = 0;

        // Calculate the percentage of dark modules
        for row in 0..25 {
            for col in 0..25 {
                if self.data[row][col] == 1 {
                    count_dark += 1;
                }
            }
        }
        let percentage_dark = count_dark * 100 / (25 * 25);

        // Take the adjacent multiples of 5 and subtract 50 from them.
        // The lower absolute value * 2 is the penalty score.
        let mut upper_multiple = 0;
        while upper_multiple < percentage_dark {
            upper_multiple += 5;
        }
        let lower_multiple = upper_multiple - 5;

        let a: i32 = lower_multiple - 50;
        let b: i32 = upper_multiple - 50;

        let x;
        if a.abs() <= b.abs() {
            x = a.abs() * 2;
        } else {
            x = b.abs() * 2;
        }

        score += x as usize;

        score
    }

    // The format string consists of error correction level,
    // mask number, and 10 error correction bits
    fn place_format_string(&mut self, mask_no: usize) {
        let mut format_string: u16;
        let mut gen_poly: u16 = 0b10100110111;
        let xor_mask: u16 = 0b101010000010010;

        // Create format string (five bits)
        // 01 for EC level L, nnn for mask number
        // Shift to MSB position
        format_string = 8 + mask_no as u16;
        format_string = format_string << 10;

        // Prepare for first division
        gen_poly = gen_poly << 3;

        // XOR (i.e. divide) until 10 EC bits remain
        while format_string.leading_zeros() < 6 {
            while gen_poly.leading_zeros() != format_string.leading_zeros() {
                gen_poly = gen_poly >> 1;
            }
            format_string = format_string ^ gen_poly;
        }

        // Add EC bits to format string
        format_string = format_string ^ ((8 + mask_no as u16) << 10);

        // Final step: XOR the resulting string with a predefined bit sequence
        format_string = format_string ^ xor_mask;

        // Extract single bits from the format string
        let mut mask: u16 = 0b0100_0000_0000_0000;
        let mut bits: [u8; 15] = [0; 15];

        for n in 0..15 {
            if format_string & mask == mask {
                bits[n] = 1;
            }
            mask = mask >> 1;
        }

        // Place format string in matrix
        for n in 0..8 {
            // Around upper left finder pattern
            if n < 6 {
                self.data[8][n] = bits[n];
                self.data[n][8] = bits[14 - n];
            }
            // Skip timing pattern
            else if n >= 6 {
                self.data[8][n + 1] = bits[n];
                self.data[1 + n][8] = bits[14 - n];
            }
            // Next to lower left and upper right finder pattern
            if n < 7 {
                self.data[24 - n][8] = bits[n];
            }
            self.data[8][24 - n] = bits[14 - n];
        }
    }

    // Return 2D matrix of modules
    pub fn export(&self) -> [[u8; 33]; 33] {
        // Add 4 modules of whitespace on all sides
        let mut qr_final: [[u8; 33]; 33] = [[0; 33]; 33];

        for row in 0..25 {
            for col in 0..25 {
                qr_final[row + 4][col + 4] = self.data[row][col];
            }
        }

        qr_final
    }
}
