use async_trait::async_trait;
use std::io::SeekFrom;
use std::path::Path;
use tokio::{fs::File, io::AsyncReadExt};

use crate::nbd;

struct FileBackedDevice {
    current_file_offs: u64,
    file: tokio::fs::File,
    block_size: u32,
    block_count: u64,
}

impl FileBackedDevice {
    fn new(block_size: u32, block_count: u64, file: File) -> Self {
        Self {
            current_file_offs: 0,
            file,
            block_size,
            block_count,
        }
    }
}

#[async_trait]
impl nbd::BlockDevice for FileBackedDevice {
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), std::io::Error> {
        if offset != self.current_file_offs {
            self.file.seek(SeekFrom::Start(offset)).await?;
            self.current_file_offs = offset;
        }
        let mut total_read = 0;
        while total_read < buf.len() {
            let rc = self.file.read(&mut buf[total_read..]).await?;
            if rc == 0 {
                break;
            }
            total_read += rc;
            self.current_file_offs += rc as u64;
        }
        if total_read < buf.len() {
            buf[total_read..].iter_mut().for_each(|v| *v = 0);
        }
        Ok(())
    }
    fn block_size(&self) -> u32 {
        self.block_size
    }
    fn block_count(&self) -> u64 {
        self.block_count
    }
}

pub async fn mount(backend_file: File, nbd_dev: &Path, block_size: u32) {
    let block_count = {
        let metadata = backend_file.metadata().await.expect("metadata");
        (metadata.len() + block_size as u64 - 1) / block_size as u64
    };

    let file_backed_device = FileBackedDevice::new(block_size, block_count, backend_file);
    nbd::new_device(nbd_dev, file_backed_device)
        .await
        .expect("mount");
}
