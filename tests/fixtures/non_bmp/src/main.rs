//! Fixture: a freeloader field written on a line that contains non-BMP
//! characters.
//!
//! The emoji before the assignment shift the field access's UTF-16 column
//! away from its `char` column. koyashi classifies `tag` as `write-only`
//! only when it maps between the two encodings; without that mapping the
//! write is matched at the wrong column, miscounted as a read, and the field
//! is mislabelled `read-only`.

#![allow(dead_code)]

struct Banner {
    tag: String,
}

fn main() {
    let mut banner = Banner {
        tag: String::new(),
    };
    let _ = "😀😀😀😀"; banner.tag = "released".to_string();
    let _ = &banner;
}
