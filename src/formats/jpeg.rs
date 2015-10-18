//! Metadata of JPEG images.

// references:
// https://en.wikipedia.org/wiki/JPEG_File_Interchange_Format
// http://dev.exiv2.org/projects/exiv2/wiki/The_Metadata_in_JPEG_files
// http://www.exif.org/Exif2-2.PDF
// http://www.codeproject.com/Articles/43665/ExifLibrary-for-NET

use std::io::{BufReader, Read, Cursor};
use std::collections::HashMap;

use byteorder::{ReadBytesExt, BigEndian, LittleEndian};

use types::{Result, Dimensions};
use traits::LoadableMetadata;
use utils::{ReadExt, BufReadExt};
//use num::rational::Ratio;

/// Represents metadata of a JPEG image.
///
/// Currently it is very basic and only provides access to image dimensions.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Metadata {
    /// Image size.
    pub dimensions: Dimensions,
    /// Image orientation.
    pub orientation: Orientation,
    // TODO: something else?
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Orientation {
    Unspecified,
    Normal, // 1
    FlippedHorizontally, // 2
    Rotated180,  // 3
    Rotated180FlippedHorizontally, // 4
    RotatedCWFippedHorizontally, // 5
    RotatedCCW, // 6
    RotatedCCWFippedHorizontally, // 7
    RotatedCW, // 8

    // TODO: better names? exiftool seems to have some
    // 1
    // 2 flip horizontal
    // 3 rotate 180
    // 4 flip vertical
    // 5 transpose
    // 6 rotate 90
    // 7 transverse
    // 8 rotate 270
    // *

    // 1 = Horizontal (normal)
    // 2 = Mirror horizontal
    // 3 = Rotate 180
    // 4 = Mirror vertical
    // 5 = Mirror horizontal and rotate 270 CW
    // 6 = Rotate 90 CW
    // 7 = Mirror horizontal and rotate 90 CW
    // 8 = Rotate 270 CW
}

impl Orientation {
    fn new(orientation: u16) -> Orientation {
        match orientation {
            1 => Orientation::Normal,
            2 => Orientation::FlippedHorizontally,
            3 => Orientation::Rotated180,
            4 => Orientation::Rotated180FlippedHorizontally,
            5 => Orientation::RotatedCWFippedHorizontally,
            6 => Orientation::RotatedCCW,
            7 => Orientation::RotatedCCWFippedHorizontally,
            8 => Orientation::RotatedCW,
            _ => Orientation::Unspecified,
        }
    }
}

#[derive(Debug)]
struct ExifSection {
    zeroth_ifd: Vec<IfdField>,
}

#[derive(Debug)]
enum IfdValue {
    Byte(u8),
    Ascii(u8),
    Short(u16),
    Long(u32),
    // TODO: fix rationals
    Rational(u32, u32),
    Undefined(u8),
    //SignedLong(i32),
    //SignedRational(Ratio<i32>),
}

//fn load_value_vector<R: ?Sized + Read>(r: &mut BufReader<&mut R>, ) {
//
//}

#[derive(Debug)]
struct IfdField {
    id: u16,
    // TODO: this doesn't express that the list is limited to a single type
    value: Vec<IfdValue>,
}

impl IfdField {
    fn load_all(r: &mut Cursor<Vec<u8>>, offset: &mut usize) -> Result<Vec<IfdField>> {
        let mut fields = vec![];
        let mut data_offsets: HashMap<u32, (u16, u16, u32)> = HashMap::new();
        let num_fields = try_if_eof!(r.read_u16::<LittleEndian>(), "while reading num_fields");
        *offset += 2;
        for _ in 0..num_fields {

            // identifies the field
            // first one seems to be "make"
            let tag = try_if_eof!(r.read_u16::<LittleEndian>(),
                                  "while reading tag");
            // the field value type
            let tag_type = try_if_eof!(r.read_u16::<LittleEndian>(),
                                       "while reading tag_type");
            // the number of values in the field
            let count_2 = try_if_eof!(r.read_u32::<LittleEndian>(),
                                      "while reading count_2");

            // next 4 bytes is either offset to value position, or the value itself, if it fits within
            // 4 bytes.
            let type_len = match tag_type {
                1 => 1, // byte
                2 => 1, // ascii
                3 => 2, // short
                4 => 4, // long
                5 => 8, // rational
                7 => 1, // undefined
                9 => 4, // slong
                10 => 8, // srational
                _ => { // unknown
                    //return Err("unknown tag type");
                    // TODO: make this raise error
                    8
                }
            };

            // TODO: try_or_eof these
            if type_len * count_2 > 4 {
                // make note to read data later
                data_offsets.insert(try!(r.read_u32::<LittleEndian>()),
                                    (tag, tag_type, count_2));
            } else {
                // read all values now
                let mut values = vec![];
                let mut values_data = [0u8; 4];
                if try!(r.read_exact_0(&mut values_data)) != values_data.len() {
                    return Err(unexpected_eof!("while reading value"));
                }
                let mut values_cursor = Cursor::new(&values_data as &[u8]);
                for _ in 0..count_2 {
                    values.push(match tag_type {
                        // TODO: implement other types
                        1 => IfdValue::Byte(try!(values_cursor.read_u8())),
                        2 => IfdValue::Ascii(try!(values_cursor.read_u8())),
                        3 => IfdValue::Short(try!(values_cursor.read_u16::<LittleEndian>())),
                        4 => IfdValue::Long(try!(values_cursor.read_u32::<LittleEndian>())),
                        _ => IfdValue::Undefined(0)
                    });
                }
                fields.push(IfdField { id: tag, value: values });
            }
            *offset += 12;
        }

        let first_ifd_offset = try_if_eof!(r.read_u32::<LittleEndian>(),
                                           "while reading first_ifd_offset");
        println!("first_ifd_offset: {}", first_ifd_offset);
        *offset += 4;

        // TODO: read values from data section
        println!("offsets: {:?}", data_offsets);
        println!("offset: {:?}", offset);
        //let sorted_data_offsets: Vec<_> = data_offsets.iter().collect().sort();
        let mut sorted_data_offsets: Vec<_> = data_offsets.iter().collect();
        sorted_data_offsets.sort();
        for (data_offset, &(tag, tag_type, count)) in sorted_data_offsets {
            println!("offset: {}", offset);
            println!("data offset: {}", data_offset);
            let empty_space = *data_offset as i32 - *offset as i32;
            if empty_space > 0 {
                for _ in 0..empty_space {
                    try!(r.read_u8());
                    *offset += 1;
                }
            } else if empty_space < 0 {
                return Err(invalid_format!("overrun"));
            }
            //if *offset != *data_offset as usize {
            //    return Err(invalid_format!("hole in data"));
            //}
            let mut values = vec![];
            for _ in 0..count {
                let res = match tag_type {
                    // TODO: implement other types
                    // TODO: need to count offset
                    1 => Ok((IfdValue::Byte(try!(r.read_u8())), 1)),
                    2 => Ok((IfdValue::Ascii(try!(r.read_u8())), 1)),
                    3 => Ok((IfdValue::Short(try!(r.read_u16::<LittleEndian>())), 2)),
                    4 => Ok((IfdValue::Long(try!(r.read_u32::<LittleEndian>())), 4)),
                    5 => Ok((IfdValue::Rational(try!(r.read_u32::<LittleEndian>()),
                                                try!(r.read_u32::<LittleEndian>())), 8)),
                    x => Err(format!("invalid tag type: {}", x))
                };
                let (ifd_value, length) = res.unwrap();
                values.push(ifd_value);
                *offset += length;
            }
            fields.push(IfdField { id: tag, value: values });
        }

        Ok(fields)
    }
}

#[derive(Debug)]
enum ByteOrder {
    BigEndian,
    LittleEndian
}

#[derive(Debug)]
struct TiffHeader {
    byte_order: ByteOrder,
    zeroth_ifd_offset: u32,
}

impl TiffHeader {
    fn load(r: &mut Cursor<Vec<u8>>) -> Result<TiffHeader> {
        let byte_order = try_if_eof!(r.read_u16::<LittleEndian>(),
                                     "while reading byte order");
        // TODO: use specified byte order
        let tiff_id = try_if_eof!(r.read_u16::<LittleEndian>(),
                                  "while reading tiff_id");
        let zeroth_ifd_offset = try_if_eof!(r.read_u32::<LittleEndian>(),
                                     "while reading zeroth ifd offset");
        match tiff_id {
            // TODO: use constant
            42 => Ok(TiffHeader {
                byte_order: try!(match byte_order {
                    0x4949 => Ok(ByteOrder::LittleEndian),
                    0x4d4d => Ok(ByteOrder::BigEndian),
                    _ => Err(invalid_format!("unknown byte order")),
                }),
                zeroth_ifd_offset: zeroth_ifd_offset,
            }),
            _ => Err(invalid_format!("unknown tiff id")),
        }
    }
}

impl ExifSection {
    fn load(r: &mut Cursor<Vec<u8>>) -> Result<ExifSection> {
        let mut identifier_code = [0u8; 6];
        if try!(r.read_exact_0(&mut identifier_code)) != identifier_code.len() {
            return Err(unexpected_eof!("while reading identifier code in exif segment"));
        }
        // TODO: add constant for this
        // identifier code should be "Exif\0\0"
        if identifier_code != [69, 120, 105, 102, 0, 0] {
            return Err(invalid_format!("not an exif segment"));
        }

        let tiff_header = try!(TiffHeader::load(r));
        // TODO: handle different endianness
        // TODO: handle zeroth_ifd_offset
        println!("{:?}", tiff_header);

        // Offset in bytes from the start of the TIFF header.
        let mut offset = 8;

        // 0th image file directory (IFD)

        let fields = try!(IfdField::load_all(r, &mut offset));
        println!("fields: {:?}", fields);

        // TODO: handle other IFDs

        Ok(ExifSection {
            zeroth_ifd: fields,
        })
    }
}

impl LoadableMetadata for Metadata {
    fn load<R: ?Sized + Read>(r: &mut R) -> Result<Metadata> {
        let mut r = &mut BufReader::new(r);
        let mut dimensions = None;
        let mut orientation = Orientation::Unspecified;
        loop {
            if try!(r.skip_until(0xff)) == 0 {
                println!("failed to skip until marker");
                return Err(unexpected_eof!("when searching for a marker"));
            }

            let marker_type = try_if_eof!(r.read_u8(), "when reading marker type");
            if marker_type == 0 { continue; }  // skip "stuffed" byte
            println!("found marker: {:x}", marker_type);

            let has_size = match marker_type {
                0xd0...0xd9 => false,
                _ => true
            };

            let size = if has_size {
                try_if_eof!(r.read_u16::<BigEndian>(), "when reading marker payload size") - 2
            } else { 0 };

            // Read entire segment into buffer with cursor.
            let mut buffer = Vec::with_capacity(size as usize);
            try!(r.take(size as u64).read_to_end(&mut buffer));
            let mut segment = Cursor::new(buffer);

            match marker_type {
                0xc0 | 0xc2 => {  // maybe others?
                    println!("found dimensions");
                    // skip one byte
                    let _ = try_if_eof!(segment.read_u8(), "when skipping to dimensions data");
                    let h = try_if_eof!(segment.read_u16::<BigEndian>(), "when reading height");
                    let w = try_if_eof!(segment.read_u16::<BigEndian>(), "when reading width");
                    dimensions = Some((w, h));
                }
                0xe1 => {  // APP1 segment (sometimes exif)
                    println!("found exif");

                    // TODO: remove unwrap
                    let exif_section = ExifSection::load(&mut segment).unwrap();
                    for ifd_field in exif_section.zeroth_ifd {
                        // TODO: figure out how to make this matching better
                        match ifd_field.id {
                            0x112 => {
                                if ifd_field.value.len() == 1 {
                                    match ifd_field.value[0] {
                                        IfdValue::Short(n) => {
                                            orientation = Orientation::new(n);
                                            println!("orientation: {:?}", orientation);
                                        }
                                        _ => { }
                                    }
                                }
                            }
                            //306 => {

                            //}
                            _ => { }
                        }

                    }
                }
                0xd9 => {  // end of image
                    break;
                }
                _ => ()
            };
        }
        Ok(Metadata {
            dimensions: dimensions.unwrap().into(),
            orientation: orientation,
        })
    }
}
