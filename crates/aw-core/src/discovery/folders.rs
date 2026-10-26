use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    PublicDocuments,
    RoamingAppData,
    ProgramData,
    LocalAppData,
    Documents,
}

#[derive(Debug, Clone)]
pub struct KnownFolders {
    pub public_documents: Option<PathBuf>,
    pub roaming_app_data: Option<PathBuf>,
    pub program_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub documents: Option<PathBuf>,
}

impl KnownFolders {
    pub fn from_system() -> io::Result<KnownFolders> {
        from_system_impl()
    }

    pub fn under(root: &Path) -> KnownFolders {
        KnownFolders {
            public_documents: Some(root.join("PublicDocuments")),
            roaming_app_data: Some(root.join("RoamingAppData")),
            program_data: Some(root.join("ProgramData")),
            local_app_data: Some(root.join("LocalAppData")),
            documents: Some(root.join("Documents")),
        }
    }

    pub fn resolve(&self, base: Base) -> Option<&Path> {
        match base {
            Base::PublicDocuments => self.public_documents.as_deref(),
            Base::RoamingAppData => self.roaming_app_data.as_deref(),
            Base::ProgramData => self.program_data.as_deref(),
            Base::LocalAppData => self.local_app_data.as_deref(),
            Base::Documents => self.documents.as_deref(),
        }
    }
}

#[cfg(windows)]
fn from_system_impl() -> io::Result<KnownFolders> {
    use windows::Win32::UI::Shell::{
        FOLDERID_Documents, FOLDERID_LocalAppData, FOLDERID_ProgramData, FOLDERID_PublicDocuments,
        FOLDERID_RoamingAppData,
    };

    let ids = [
        &FOLDERID_PublicDocuments,
        &FOLDERID_RoamingAppData,
        &FOLDERID_ProgramData,
        &FOLDERID_LocalAppData,
        &FOLDERID_Documents,
    ];
    let mut errors = Vec::new();
    let [
        public_documents,
        roaming_app_data,
        program_data,
        local_app_data,
        documents,
    ] = ids.map(|id| known_folder(id).map_err(|error| errors.push(error)).ok());
    if errors.len() == ids.len() {
        return Err(errors.remove(0));
    }
    Ok(KnownFolders {
        public_documents,
        roaming_app_data,
        program_data,
        local_app_data,
        documents,
    })
}

#[cfg(not(windows))]
fn from_system_impl() -> io::Result<KnownFolders> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(windows)]
fn known_folder(id: &windows::core::GUID) -> io::Result<PathBuf> {
    use std::ffi::{OsString, c_void};
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath};

    // SAFETY: `id` is a valid GUID for the call, DONT_VERIFY flag, null token means current user.
    let raw =
        unsafe { SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, None) }.map_err(io::Error::from)?;

    // SAFETY: `raw` is a NUL-terminated UTF-16 string we own until the `CoTaskMemFree` below.
    let path = PathBuf::from(OsString::from_wide(unsafe { raw.as_wide() }));

    // SAFETY: `raw` came from `SHGetKnownFolderPath`, is freed exactly once here, never reused.
    unsafe { CoTaskMemFree(Some(raw.as_ptr().cast::<c_void>().cast_const())) };

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn under_builds_fake_layout() {
        let f = KnownFolders::under(Path::new("X"));
        let x = Path::new("X");
        assert_eq!(
            f.resolve(Base::PublicDocuments),
            Some(x.join("PublicDocuments").as_path())
        );
        assert_eq!(
            f.resolve(Base::RoamingAppData),
            Some(x.join("RoamingAppData").as_path())
        );
        assert_eq!(
            f.resolve(Base::ProgramData),
            Some(x.join("ProgramData").as_path())
        );
        assert_eq!(
            f.resolve(Base::LocalAppData),
            Some(x.join("LocalAppData").as_path())
        );
        assert_eq!(
            f.resolve(Base::Documents),
            Some(x.join("Documents").as_path())
        );
    }

    #[cfg(windows)]
    #[test]
    fn from_system_returns_existing_directories() {
        let f = KnownFolders::from_system().unwrap();
        for dir in [
            &f.public_documents,
            &f.roaming_app_data,
            &f.program_data,
            &f.local_app_data,
            &f.documents,
        ] {
            let dir = dir.as_deref().expect("known folder resolved");
            assert!(dir.is_dir(), "{}", dir.display());
        }
    }
}
