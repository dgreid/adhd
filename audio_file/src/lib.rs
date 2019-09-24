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
    SubsectionId(u32),
}
pub type Result<T> = std::result::Result<T, Error>;

const RIFF_ID: u32 = 0x5249_4646; // "RIFF" in ASCII,
const WAVE_ID: u32 = 0x5741_5645; // "WAVE" in ASCII,
const FMT_ID: u32 = 0x666d_7420; // "fmt " in ASCII,
const DATA_ID: u32 = 0x6461_7461; // "data" in ASCII,

const PCM_FMT: u16 = 1;

struct AudioFormat {
    audio_format: u16,
    num_channels: u16,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

pub struct WavFile<F>
where
    F: Read + Seek,
{
    inner: F,
    data_section_size: usize,
    format: AudioFormat,
}

impl<F> WavFile<F>
where
    F: Read + Seek,
{
    /// Creates a `WavFile` from the given raw `File`.
    /// A WAVE file that doesn't container PCM data will result in an error.
    pub fn from_raw(mut inner: F) -> Result<WavFile<F>> {
        inner.seek(SeekFrom::Start(0)).map_err(Error::SeekingFile)?;

        // Read the main chunk.
        let chunk_id = read_be_u32(&mut inner).map_err(Error::ReadingChunkDescriptor)?;
        if chunk_id != RIFF_ID {
            return Err(Error::InvalidChunkId);
        }
        let _chunk_size = read_le_u32(&mut inner).map_err(Error::ReadingChunkDescriptor)?;
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
        let data_section_size = read_le_u32(&mut inner).map_err(Error::ReadingDataHeader)? as usize;
        if data_section_id != DATA_ID {
            return Err(Error::SubsectionId(data_section_id));
        }

        Ok(WavFile {
            inner,
            data_section_size,
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

    /// Returns the number of comlete audio frames in the file. A frame is defined as one sample
    /// from each channel. For example a file with 128 bytes of data would have 32 stereo frames of
    /// 16 bit data ( 128 bytes/file / ( 2 bytes/sample * 2 samples/frame ) ) = 32 frames/file.
    pub fn num_frames(&self) -> usize {
        self.data_section_size as usize / self.format.block_align as usize
    }

    /// Returns the number of bits in each sample
    pub fn bits_per_sample(&self) -> usize {
        self.format.bits_per_sample as usize
    }
}

fn read_be_u32<F: Read>(f: &mut F) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf[..])?;
    Ok(u32::from_be_bytes(buf))
}

fn read_le_u32<F: Read>(f: &mut F) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf[..])?;
    Ok(u32::from_le_bytes(buf))
}

fn read_le_u16<F: Read>(f: &mut F) -> std::io::Result<u16> {
    let mut buf = [0u8; 2];
    f.read_exact(&mut buf[..])?;
    Ok(u16::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FILE_HEADER: [u8; 44] = [
        0x52, 0x49, 0x46, 0x46, 0x04, 0x6c, 0x21, 0x02, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6d, 0x74,
        0x20, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x44, 0xac, 0x00, 0x00, 0x10, 0xb1,
        0x02, 0x00, 0x04, 0x00, 0x10, 0x00, 0x64, 0x61, 0x74, 0x61, 0x00, 0x01, 0x00, 0x00,
    ];

    #[test]
    fn header_read() {
        let header = Cursor::new(&FILE_HEADER[..]);
        let wav_file = match WavFile::from_raw(header) {
            Ok(w) => w,
            Err(_) => panic!("Failed to create wav file"),
        };
        assert_eq!(wav_file.format.sample_rate, 44100);
        assert_eq!(wav_file.format.num_channels, 2);
        assert_eq!(wav_file.format.bits_per_sample, 16);
        assert_eq!(wav_file.format.block_align, 4);
        assert_eq!(wav_file.format.audio_format, 1);
        assert_eq!(wav_file.data_section_size, 0x100);
        assert_eq!(
            wav_file.format.byte_rate,
            wav_file.format.sample_rate * wav_file.format.num_channels as u32 * 2
        );

        assert_eq!(wav_file.bits_per_sample(), 16);
        // 256 bytes per file(data_section_size) / 4 bytes per frame
        assert_eq!(wav_file.num_frames(), 64);
    }
}
