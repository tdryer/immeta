//! Metadata of JPEG images.

// references:
// https://en.wikipedia.org/wiki/JPEG_File_Interchange_Format
// http://dev.exiv2.org/projects/exiv2/wiki/The_Metadata_in_JPEG_files
// http://www.exif.org/Exif2-2.PDF
// http://www.codeproject.com/Articles/43665/ExifLibrary-for-NET

use std::io::{BufReader, Read, Cursor};
use std::collections::HashMap;

use byteorder;
use byteorder::{ReadBytesExt, BigEndian, LittleEndian};

use types::{Result, Dimensions};
use traits::LoadableMetadata;
use utils::{ReadExt, BufReadExt};

fn read_u16<R: Read>(byte_order: ByteOrder, buf: &mut R) -> byteorder::Result<u16> {
    match byte_order {
        ByteOrder::LittleEndian => buf.read_u16::<LittleEndian>(),
        ByteOrder::BigEndian => buf.read_u16::<BigEndian>(),
    }
}

fn read_u32<R: Read>(byte_order: ByteOrder, buf: &mut R) -> byteorder::Result<u32> {
    match byte_order {
        ByteOrder::LittleEndian => buf.read_u32::<LittleEndian>(),
        ByteOrder::BigEndian => buf.read_u32::<BigEndian>(),
    }
}

/// Represents metadata of a JPEG image.
///
/// Currently it is very basic and only provides access to image dimensions.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Metadata {
    /// Image size.
    pub dimensions: Dimensions,
    /// Image orientation.
    pub orientation: Orientation,
    /// File change date and time.
    // TODO: parse this
    pub date_time: Option<String>,
    /// Image input equipment manufacturer.
    pub make: Option<String>,
    /// Image input equipment model.
    pub model: Option<String>,
    // TODO: something else?
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Orientation {
    Normal,
    MirrorHorizontal,
    Rotate180,
    FlipVertical,
    Transpose,
    Rotate90,
    Transverse,
    Rotate270,
    Unspecified,
}

impl Orientation {
    fn new(orientation: u16) -> Orientation {
        match orientation {
            1 => Orientation::Normal,
            2 => Orientation::MirrorHorizontal,
            3 => Orientation::Rotate180,
            4 => Orientation::FlipVertical,
            5 => Orientation::Transpose,
            6 => Orientation::Rotate90,
            7 => Orientation::Transverse,
            8 => Orientation::Rotate270,
            _ => Orientation::Unspecified,
        }
    }
}

#[derive(Debug)]
struct ExifSection {
    zeroth_ifd: Vec<Tag>,
}

#[derive(Clone,Debug)]
enum TagDatatype {
    Byte,
    Ascii,
    Short,
    Long,
    Rational,
    Undefined,
    SignedLong,
    SignedRational,
}

impl TagDatatype {
    fn new(datatype: u16) -> Result<TagDatatype> {
        match datatype {
            1 => Ok(TagDatatype::Byte),
            2 => Ok(TagDatatype::Ascii),
            3 => Ok(TagDatatype::Short),
            4 => Ok(TagDatatype::Long),
            5 => Ok(TagDatatype::Rational),
            7 => Ok(TagDatatype::Undefined),
            9 => Ok(TagDatatype::SignedLong),
            10 => Ok(TagDatatype::SignedRational),
            _ => Err(invalid_format!("invalid tag datatype: {}", datatype))
        }
    }

    fn len(self: &TagDatatype) -> usize {
        match *self {
            TagDatatype::Byte => 1,
            TagDatatype::Ascii => 1,
            TagDatatype::Short => 2,
            TagDatatype::Long => 4,
            TagDatatype::Rational => 8,
            TagDatatype::Undefined => 1,
            TagDatatype::SignedLong => 4,
            TagDatatype::SignedRational => 8,
        }
    }
}

#[derive(Debug)]
struct Tag {
    id: u16,
    datatype: TagDatatype,
    data: Vec<u8>,
    byte_order: ByteOrder,
}

impl Tag {
    fn new(id: u16, datatype: TagDatatype, data: Vec<u8>, byte_order: ByteOrder) -> Tag {
        Tag { id: id, datatype: datatype, data: data, byte_order: byte_order }
    }

    fn get_short(self: &Tag) -> Result<u16> {
        let mut c: &[u8] = &self.data;
        match (&self.datatype, self.data.len()) {
            (&TagDatatype::Short, 2) => Ok(try_if_eof!(read_u16(self.byte_order, &mut c),
                                           "this should never happen")),
            _ => Err(invalid_format!("tag has invalid datatype or count"))
        }
    }

    fn get_ascii(self: &Tag) -> Result<String> {
        let mut new_data = self.data.clone();
        // Remove trailing null from string.
        new_data.pop();
        match self.datatype {
            TagDatatype::Ascii => (String::from_utf8(new_data)
                                   .or(Err(invalid_format!("invalid string")))),
            _ => Err(invalid_format!("tag has invalid datatype"))
        }
    }

    fn load_all(r: &mut Cursor<Vec<u8>>, offset: &mut usize, byte_order: ByteOrder)
            -> Result<Vec<Tag>> {
        let mut fields = vec![];
        let mut data_offsets: HashMap<u32, (u16, TagDatatype, usize)> = HashMap::new();
        let num_fields = try_if_eof!(read_u16(byte_order, r),
                                     "while reading num_fields");
        println!("will load {} tags", num_fields);
        *offset += 2;
        for _ in 0..num_fields {

            // identifies the field
            // first one seems to be "make"
            let tag_id = try_if_eof!(read_u16(byte_order, r),
                                  "while reading tag");
            // the field value type
            let tag_datatype = try!(TagDatatype::new(try_if_eof!(read_u16(byte_order, r),
                                                     "while reading tag_type")));
            // the number of values in the field
            let count_2 = try_if_eof!(read_u32(byte_order, r),
                                      "while reading count_2") as usize;
            println!("found tag {} of type {:?} containing {} values", tag_id, tag_datatype, count_2);

            // next 4 bytes is either offset to value position, or the value itself, if it fits within
            // 4 bytes.
            let type_len = tag_datatype.len();

            // TODO: try_or_eof these
            if type_len * count_2 > 4 {
                // make note to read data later
                data_offsets.insert(try!(read_u32(byte_order, r)),
                                    (tag_id, tag_datatype, count_2));
            } else {
                // read all values now
                let mut values_data = Vec::with_capacity(4);
                try!(r.take(4).read_to_end(&mut values_data));
                values_data.truncate(type_len * count_2);
                fields.push(Tag::new(tag_id, tag_datatype, values_data, byte_order));
            }
            *offset += 12;
        }

        let first_ifd_offset = try_if_eof!(read_u32(byte_order, r),
                                           "while reading first_ifd_offset");
        println!("first_ifd_offset: {}", first_ifd_offset);
        *offset += 4;

        // read values from data section
        //let sorted_data_offsets: Vec<_> = data_offsets.iter().collect().sort();
        let mut sorted_data_offsets: Vec<_> = data_offsets.iter().collect();
        sorted_data_offsets.sort_by(|&(offset_a, _), &(offset_b, _)| offset_a.cmp(offset_b));
        for (data_offset, &(tag, ref tag_datatype, count)) in sorted_data_offsets {
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

            let data_len = tag_datatype.len() * count;
            let mut data = Vec::with_capacity(data_len as usize);
            try!(r.take(data_len as u64).read_to_end(&mut data));
            *offset += data_len;
            fields.push(Tag::new(tag, tag_datatype.clone(), data, byte_order));
        }

        Ok(fields)
    }
}

#[derive(Debug, Clone, Copy)]
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
        let byte_order_id = try_if_eof!(r.read_u16::<LittleEndian>(),
                                        "while reading byte order");
        let byte_order = try!(match byte_order_id {
            0x4949 => Ok(ByteOrder::LittleEndian),
            0x4d4d => Ok(ByteOrder::BigEndian),
            _ => Err(invalid_format!("unknown byte order id: {:x}", byte_order_id)),
        });
        let tiff_id = try_if_eof!(read_u16(byte_order, r),
                                  "while reading tiff id");
        let zeroth_ifd_offset = try_if_eof!(read_u32(byte_order, r),
                                            "while reading zeroth IFD offset");
        match tiff_id {
            // TODO: use constant
            42 => Ok(TiffHeader {
                byte_order: byte_order,
                zeroth_ifd_offset: zeroth_ifd_offset,
            }),
            _ => Err(invalid_format!("unknown tiff id: {}", tiff_id)),
        }
    }
}

impl ExifSection {
    fn load(r: &mut Cursor<Vec<u8>>) -> Result<ExifSection> {
        // TODO: add constant for this
        // identifier code should be "Exif\0\0"
        let mut identifier_code = [0u8; 6];
        if try!(r.read_exact_0(&mut identifier_code)) != identifier_code.len() {
            return Err(unexpected_eof!("while reading identifier code in exif segment"));
        }
        if identifier_code != [69, 120, 105, 102, 0, 0] {
            return Err(invalid_format!("not an exif segment: {:?}", identifier_code));
        }

        let tiff_header = try!(TiffHeader::load(r));
        // TODO: handle different endianness
        // TODO: handle zeroth_ifd_offset
        println!("{:?}", tiff_header);

        // Offset in bytes from the start of the TIFF header.
        let mut offset = 8;

        // 0th image file directory (IFD)

        let fields = try!(Tag::load_all(r, &mut offset, tiff_header.byte_order));
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
        let mut date_time = None;
        let mut make = None;
        let mut model = None;
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
                    println!("dimensions: {:?}", dimensions);
                }
                0xe1 => {  // APP1 segment (sometimes exif)
                    println!("found exif");

                    // TODO: remove unwrap
                    let exif_section = ExifSection::load(&mut segment);
                    match exif_section {
                        Ok(exif_section) => {
                            for ifd_field in exif_section.zeroth_ifd {
                                match ifd_field.id {
                                    0x112 => { orientation = Orientation::new(try!(ifd_field.get_short())); },
                                    306 => { date_time = Some(try!(ifd_field.get_ascii())); },
                                    271 => { make = Some(try!(ifd_field.get_ascii())); },
                                    272 => { model = Some(try!(ifd_field.get_ascii())); },
                                    x => { println!("unknown tag id: {}", x); }
                                };

                            }
                        }
                        Err(e) => {
                            println!("skipping invalid exif section: {}", e);
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
            date_time: date_time,
            make: make,
            model: model,
        })
    }
}
