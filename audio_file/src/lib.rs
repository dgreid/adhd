use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub enum Error {
    InvalidAudioFormat(u16),
    InvalidChunkId,
    InvalidFormat,
    InvalidPcmFormatChunkSize(u32),
    InvalidSubChunkId,
    ReadingChunkDescriptor(std::io::Error),
    ReadingDataHeader(std::io::Error),
    ReadingFormatChunk(std::io::Error),
    SeekingFile(std::io::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

const RIFF_ID: u32 = 0x52494646; // "RIFF" in ASCII,
const WAVE_ID: u32 = 0x57415645; // "WAVE" in ASCII,
const FMT_ID: u32 = 0x666d7420; // "fmt " in ASCII,

const PCM_FMT: u16 = 1;

struct AudioFormat {
    audio_format: u16,
    num_channels: u16,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

pub struct WavFile {
    inner: File,
    format: AudioFormat,
}

impl WavFile {
    /// Creates a `WavFile` from the given raw `File`.
    /// A WAVE file that doesn't container PCM data will result in an error.
    pub fn from_raw(mut inner: File) -> Result<WavFile> {
        inner.seek(SeekFrom::Start(0)).map_err(Error::SeekingFile)?;

        // Read the main chunk.
        let chunk_id = read_be_u32(&mut inner).map_err(Error::ReadingChunkDescriptor)?;
        if chunk_id != RIFF_ID {
            return Err(Error::InvalidChunkId);
        }
        let chunk_size = read_le_u32(&mut inner).map_err(Error::ReadingChunkDescriptor)?;
        let riff_format = read_be_u32(&mut inner).map_err(Error::ReadingChunkDescriptor)?;
        if riff_format != WAVE_ID {
            return Err(Error::InvalidFormat);
        }

        // Read the format subchunk that describes a WAVE file.
        let subchunk_id = read_be_u32(&mut inner).map_err(Error::ReadingFormatChunk)?;
        if subchunk_id != FMT_ID {
            return Err(Error::InvalidSubChunkId);
        }
        let subchunk_size = read_le_u32(&mut inner).map_err(Error::ReadingFormatChunk)?;
        if subchunk_size != 16 {
            return Err(Error::InvalidPcmFormatChunkSize(subchunk_size));
        }

        let audio_format = read_le_u16(&mut inner).map_err(Error::ReadingFormatChunk)?;
        if audio_format != PCM_FMT {
            return Err(Error::InvalidAudioFormat(audio_format));
        }

        let num_channels = read_le_u16(&mut inner).map_err(Error::ReadingFormatChunk)?;
        let sample_rate = read_le_u32(&mut inner).map_err(Error::ReadingFormatChunk)?;
        let byte_rate = read_le_u32(&mut inner).map_err(Error::ReadingFormatChunk)?;
        let block_align = read_le_u16(&mut inner).map_err(Error::ReadingFormatChunk)?;
        let bits_per_sample = read_le_u16(&mut inner).map_err(Error::ReadingFormatChunk)?;

        // Read the data section header

        let data_section_id = read_be_u32(&mut inner).map_err(Error::ReadingDataHeader)?;
        let data_section_size = read_le_u32(&mut inner).map_err(Error::ReadingDataHeader)?;

        Ok(WavFile {
            inner,
            format: AudioFormat {
                audio_format,
                num_channels,
                sample_rate,
                byte_rate,
                block_align,
                bits_per_sample,
            },
        })
    }
}

fn read_be_u32(f: &mut File) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    f.read(&mut buf[..])?;
    Ok(u32::from_be_bytes(buf))
}

fn read_le_u32(f: &mut File) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    f.read(&mut buf[..])?;
    Ok(u32::from_le_bytes(buf))
}

fn read_le_u16(f: &mut File) -> std::io::Result<u16> {
    let mut buf = [0u8; 2];
    f.read(&mut buf[..])?;
    Ok(u16::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
