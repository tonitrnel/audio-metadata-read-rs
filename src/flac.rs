use crate::reader::Reader;
use crate::utils::{debug_vec, ByteReader};
use crate::vorbis_comment::VorbisComment;
use std::fmt::{Debug, Formatter};

const FLAC_SIGNATURE: [u8; 4] = [0x66, 0x4c, 0x61, 0x43];

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct Flac {
    blocks: Vec<FlacParsedBlock>,
}

impl Reader for Flac {
    #[allow(unused)]
    fn from_bytes(bytes: &[u8]) -> Self {
        if !Flac::is(bytes) {
            panic!("Invalid flac audio format.");
        }
        let mut bytes = ByteReader::with_offset(bytes, 4);
        let mut blocks: Vec<FlacParsedBlock> = Vec::new();
        loop {
            let block = Block::new(&mut bytes);
            let is_last = block.is_last;
            match block {
                block if StreamInfo::is_stream_info(&block) => {
                    blocks.push(FlacParsedBlock::StreamInfo(StreamInfo::new(block)))
                }
                block if Picture::is_picture(&block) => {
                    blocks.push(FlacParsedBlock::Picture(Picture::new(block)))
                }
                block if Comments::is_comment(&block) => {
                    blocks.push(FlacParsedBlock::Comment(Comments::new(block).inner))
                }
                _ => blocks.push(FlacParsedBlock::Raw(block)),
            }
            if is_last || bytes.is_end() {
                break;
            }
        }
        Self { blocks }
    }
    fn is(bytes: &[u8]) -> bool {
        bytes[0..4] == FLAC_SIGNATURE
    }
}

#[allow(unused)]
pub(crate) struct Block {
    id: u8,
    is_last: bool,
    len: usize,
    data: Vec<u8>,
}

impl Debug for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Block")
            .field("id", &self.id)
            .field("is_last", &self.is_last)
            .field("len", &self.len)
            .field("data", &debug_vec(&self.data))
            .finish()
    }
}

impl Block {
    pub(crate) fn new(bytes: &mut ByteReader) -> Self {
        let (is_last, id) = if bytes.peek(1)[0] >> 7 == 1 {
            // 去掉标志位
            (true, bytes.read_next_u8() & 0xf)
        } else {
            (false, bytes.read_next_u8())
        };
        let len = ((bytes.read_next_u8() as usize) << 16)
            | ((bytes.read_next_u8() as usize) << 8)
            | (bytes.read_next_u8() as usize);
        let data = bytes.read(len);
        Self {
            id,
            is_last,
            len,
            data: data.to_vec(),
        }
    }
}

#[derive(Debug)]
enum FlacParsedBlock {
    StreamInfo(StreamInfo),
    Comment(VorbisComment),
    Picture(Picture),
    Raw(Block),
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct StreamInfo {
    minimum_block_size: u32,
    maximum_block_size: u32,
    minimum_frame_size: u32,
    maximum_frame_size: u32,
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
    total_samples: u64,
    md5: String,
}

impl StreamInfo {
    pub(crate) fn new(block: Block) -> Self {
        let mut bytes = ByteReader::new(&block.data);
        let minimum_block_size = bytes.read_next_u16(true) as u32;
        let maximum_block_size = bytes.read_next_u16(true) as u32;
        let (minimum_frame_size, maximum_frame_size) = {
            let bytes = bytes.read(6);
            let minimum_frame_size =
                ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32;
            let maximum_frame_size =
                ((bytes[3] as u32) << 16) | ((bytes[4] as u32) << 8) | bytes[5] as u32;
            (minimum_frame_size, maximum_frame_size)
        };
        let (sample_rate, channels, bits_per_sample, total_samples) = {
            let bytes = bytes.read(8);
            let sample_rate =
                ((bytes[0] as u32) << 12) | ((bytes[1] as u32) << 4) | (bytes[2] >> 4) as u32;
            let channels = ((bytes[2] & 0x0e) >> 1) + 1;
            let bits_per_sample = (((bytes[2] & 0x01) << 4) | (bytes[3] >> 4)) + 1;
            let total_samples = (((bytes[3] & 0x0f) as u64) << 32)
                | ((bytes[4] as u64) << 24)
                | ((bytes[5] as u64) << 16)
                | ((bytes[6] as u64) << 8)
                | (bytes[7] as u64);
            (sample_rate, channels, bits_per_sample, total_samples)
        };
        let md5 = bytes
            .read(16)
            .iter()
            .map(|it| format!("{:x}", it))
            .collect::<String>();
        Self {
            minimum_block_size,
            maximum_block_size,
            minimum_frame_size,
            maximum_frame_size,
            sample_rate,
            channels,
            bits_per_sample,
            total_samples,
            md5,
        }
    }
    pub(crate) fn is_stream_info(block: &Block) -> bool {
        block.id == 0x00
    }
}

#[allow(unused)]
pub(crate) struct Picture {
    r#type: u8,
    mime: String,
    desc: String,
    len: u32,
    width: u32,
    height: u32,
    color_depth: u32,
    indexed_color: u32,
    data: Vec<u8>,
}

impl Debug for Picture {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("type", &self.r#type)
            .field("mime", &self.mime)
            .field("desc", &self.desc)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("color_depth", &self.color_depth)
            .field("indexed_color", &self.indexed_color)
            .field("len", &self.len)
            .field("data", &format!("[..]({})", self.data.len()))
            .finish()
    }
}

impl Picture {
    pub(crate) fn new(block: Block) -> Self {
        let mut bytes = ByteReader::new(&block.data);
        // type
        let r#type = bytes.read_next_u32(true) as u8;
        // mime
        let mime_length = bytes.read_next_u32(true) as usize;
        let mime = bytes.read_uft8_string(mime_length);
        // desc
        let desc_length = bytes.read_next_u32(true) as usize;
        let desc = bytes.read_uft8_string(desc_length);
        // width
        let width = bytes.read_next_u32(true);
        let height = bytes.read_next_u32(true);
        let color_depth = bytes.read_next_u32(true);
        let indexed_color = bytes.read_next_u32(true);
        let len = bytes.read_next_u32(true);
        let data = bytes.read_remaining().to_vec();
        Picture {
            r#type,
            mime,
            len,
            desc,
            width,
            height,
            color_depth,
            indexed_color,
            data,
        }
    }
    pub(crate) fn is_picture(block: &Block) -> bool {
        block.id == 0x06
    }
    #[allow(unused)]
    pub(crate) fn get_ext(&self) -> String {
        self.mime
            .splitn(2, '/')
            .skip(1)
            .take(1)
            .map(|it| format!(".{}", it))
            .collect::<String>()
    }
}

#[derive(Debug)]
pub(crate) struct Comments {
    inner: VorbisComment,
}

impl Comments {
    pub(crate) fn new(block: Block) -> Self {
        Self {
            inner: VorbisComment::new(&block.data),
        }
    }
    pub(crate) fn is_comment(block: &Block) -> bool {
        block.id == 0x04
    }
}
