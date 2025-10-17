use {
    crate::{
        devices::BlockDevice,
        fs::{Error, Filesystem},
    },
    alloc::{borrow::ToOwned, string::String, vec::Vec},
    tar_no_std::{ArchiveEntry, TarArchive},
};

pub struct TarFilesystem<'device, B> {
    _dev: &'device mut B,
    archive: TarArchive,
}

impl<'device, B: BlockDevice> TarFilesystem<'device, B> {
    pub fn mount(dev: &'device mut B) -> Self {
        // read entire file into memory and create tar archive
        let archive = {
            let mut buf = alloc::vec![0u8; dev.size()];

            // workaround for https://github.com/rcore-os/virtio-drivers/issues/135

            // 1GiB chunks
            const G: usize = 1024 * 1024 * 1024;
            buf.chunks_mut(G).enumerate().for_each(|(i, chunk)| {
                let block_index = (i * G) / dev.block_size();
                dev.read(chunk, block_index).unwrap();
            });

            TarArchive::new(buf.into()).unwrap()
        };

        Self { _dev: dev, archive }
    }
}

impl<'device, B: BlockDevice> Filesystem for TarFilesystem<'device, B> {
    fn list<S: AsRef<str>>(&mut self, _directory: S) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn size<S: AsRef<str>>(&mut self, filename: S) -> Result<usize, Error> {
        let entry = self
            .archive
            .entries()
            .find(|e| e.filename().as_str().unwrap() == (filename.as_ref().trim_start_matches('/')))
            .ok_or(Error::NotFound(filename.as_ref().to_owned()))?;

        Ok(entry.size())
    }

    fn read_to_vec<S: AsRef<str>>(&mut self, filename: S) -> Result<Vec<u8>, Error> {
        let entry = self
            .archive
            .entries()
            .find(|e| e.filename().as_str().unwrap() == (filename.as_ref().trim_start_matches('/')))
            .ok_or(Error::NotFound(filename.as_ref().to_owned()))?;

        let mut buffer = alloc::vec![0; entry.size()];

        buffer.copy_from_slice(entry.data());

        Ok(buffer)
    }
}

pub struct TarFile<'fs> {
    entry: ArchiveEntry<'fs>,
}
