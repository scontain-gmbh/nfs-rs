// Copyright 2025 NetApp Inc. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use nfs4::FileAttribute::{
    CanSetTime as FileAttributeCanSetTime, CaseInsensitive as FileAttributeCaseInsensitive,
    CasePreserving as FileAttributeCasePreserving, ChownRestricted as FileAttributeChownRestricted,
    FileHandle as FileAttributeFileHandle, FileId as FileAttributeFileId,
    FilesAvail as FileAttributeFilesAvail, FilesFree as FileAttributeFilesFree,
    FilesTotal as FileAttributeFilesTotal, FsId as FileAttributeFsId,
    Homogeneous as FileAttributeHomogeneous, LinkSupport as FileAttributeLinkSupport,
    MaxFileSize as FileAttributeMaxFileSize, MaxLink as FileAttributeMaxLink,
    MaxName as FileAttributeMaxName, MaxRead as FileAttributeMaxRead,
    MaxWrite as FileAttributeMaxWrite, Mode as FileAttributeFileMode,
    NoTrunc as FileAttributeNoTrunc, NumLinks as FileAttributeNumLinks,
    Owner as FileAttributeOwner, OwnerGroup as FileAttributeOwnerGroup,
    RawDev as FileAttributeRawDev, Size as FileAttributeFileSize,
    SpaceAvail as FileAttributeSpaceAvail, SpaceFree as FileAttributeSpaceFree,
    SpaceTotal as FileAttributeSpaceTotal, SpaceUsed as FileAttributeSpaceUsed,
    SymlinkSupport as FileAttributeSymlinkSupport, TimeAccess as FileAttributeTimeAccess,
    TimeAccessSet as FileAttributeTimeAccessSet, TimeCreate as FileAttributeTimeCreate,
    TimeDelta as FileAttributeTimeDelta, TimeModify as FileAttributeTimeModify,
    TimeModifySet as FileAttributeTimeModifySet, Type as FileAttributeFileType,
};
use nfs4::FileAttributeId::{
    CanSetTime as FileAttributeIdCanSetTime, CaseInsensitive as FileAttributeIdCaseInsensitive,
    CasePreserving as FileAttributeIdCasePreserving,
    ChownRestricted as FileAttributeIdChownRestricted, FileHandle as FileAttributeIdFileHandle,
    FileId as FileAttributeIdFileId, FilesAvail as FileAttributeIdFilesAvail,
    FilesFree as FileAttributeIdFilesFree, FilesTotal as FileAttributeIdFilesTotal,
    FsId as FileAttributeIdFsId, Homogeneous as FileAttributeIdHomogeneous,
    LinkSupport as FileAttributeIdLinkSupport, MaxFileSize as FileAttributeIdMaxFileSize,
    MaxLink as FileAttributeIdMaxLink, MaxName as FileAttributeIdMaxName,
    MaxRead as FileAttributeIdMaxRead, MaxWrite as FileAttributeIdMaxWrite,
    Mode as FileAttributeIdFileMode, NoTrunc as FileAttributeIdNoTrunc,
    NumLinks as FileAttributeIdNumLinks, Owner as FileAttributeIdOwner,
    OwnerGroup as FileAttributeIdOwnerGroup, RawDev as FileAttributeIdRawDev,
    Size as FileAttributeIdFileSize, SpaceAvail as FileAttributeIdSpaceAvail,
    SpaceFree as FileAttributeIdSpaceFree, SpaceTotal as FileAttributeIdSpaceTotal,
    SpaceUsed as FileAttributeIdSpaceUsed, SymlinkSupport as FileAttributeIdSymlinkSupport,
    TimeAccess as FileAttributeIdTimeAccess, TimeCreate as FileAttributeIdTimeCreate,
    TimeDelta as FileAttributeIdTimeDelta, TimeModify as FileAttributeIdTimeModify,
    Type as FileAttributeIdFileType,
};
use nfs4::{FileAttributes, Mode as FileMode, SetTime as FileSetTime};

use crate::{split_path, Error, NFSVersion, ObjRes, Result, SocketAddr, TcpStream, Time};
use std::net::ToSocketAddrs;

const ROOT_PATH: &str = "/";

const FSF_LINK: u32 = 0x0001; // File system supports hard links

const FSF_SYMLINK: u32 = 0x0002; // File system supports symbolic links

const FSF_HOMOGENEOUS: u32 = 0x0008; // File system objects all return same pathconf

const FSF_CANSETTIME: u32 = 0x0010; // File system supports setting times via setattr

macro_rules! get_value_from_file_attributes {
    ($file_attributes:ident, $attr_id:ident, $attr_type:ident, $from_value:ident) => {
        match $file_attributes.get($attr_id) {
            Some($attr_type(value)) => $from_value(&value),
            _ => Default::default(),
        }
    };
}

impl Into<crate::mount::FSInfo> for nfs4::GetAttrRes {
    fn into(self) -> crate::mount::FSInfo {
        self.object_attributes.into()
    }
}

impl Into<crate::mount::FSInfo> for nfs4::FileAttributes {
    fn into(self) -> crate::mount::FSInfo {
        let max_read: u64 = get_value_from_file_attributes!(
            self,
            FileAttributeIdMaxRead,
            FileAttributeMaxRead,
            from_ref
        );

        let max_write: u64 = get_value_from_file_attributes!(
            self,
            FileAttributeIdMaxWrite,
            FileAttributeMaxWrite,
            from_ref
        );

        let link_support: bool = get_value_from_file_attributes!(
            self,
            FileAttributeIdLinkSupport,
            FileAttributeLinkSupport,
            from_ref
        );

        let symlink_support: bool = get_value_from_file_attributes!(
            self,
            FileAttributeIdSymlinkSupport,
            FileAttributeSymlinkSupport,
            from_ref
        );

        let homogeneous: bool = get_value_from_file_attributes!(
            self,
            FileAttributeIdHomogeneous,
            FileAttributeHomogeneous,
            from_ref
        );

        let can_set_time: bool = get_value_from_file_attributes!(
            self,
            FileAttributeIdCanSetTime,
            FileAttributeCanSetTime,
            from_ref
        );

        let mut properties = 0;

        if link_support {
            properties = properties | FSF_LINK;
        }

        if symlink_support {
            properties = properties | FSF_SYMLINK;
        }

        if homogeneous {
            properties = properties | FSF_HOMOGENEOUS;
        }

        if can_set_time {
            properties = properties | FSF_CANSETTIME;
        }

        crate::mount::FSInfo {
            rtmax: max_read as u32,

            rtpref: max_read as u32,

            rtmult: max_read as u32,

            wtmax: max_write as u32,

            wtpref: max_write as u32,

            wtmult: max_write as u32,

            dtpref: 1000, // FIXME: magic number taken from nfs4_client::Client::read_dir where value is hardcoded

            maxfilesize: get_value_from_file_attributes!(
                self,
                FileAttributeIdMaxFileSize,
                FileAttributeMaxFileSize,
                from_ref
            ),

            time_delta: get_value_from_file_attributes!(
                self,
                FileAttributeIdTimeDelta,
                FileAttributeTimeDelta,
                from_time
            ),

            properties,

            attr: Some(self.into()),
        }
    }
}

impl Into<crate::mount::FSStat> for nfs4::GetAttrRes {
    fn into(self) -> crate::mount::FSStat {
        self.object_attributes.into()
    }
}

impl Into<crate::mount::FSStat> for nfs4::FileAttributes {
    fn into(self) -> crate::mount::FSStat {
        let invarsec: u32 = 0; // FIXME: don't know if any file attribute matches NFSv3's invarsec (number of seconds for which the file system is not expected to change)

        crate::mount::FSStat {
            tbytes: get_value_from_file_attributes!(
                self,
                FileAttributeIdSpaceTotal,
                FileAttributeSpaceTotal,
                from_ref
            ),

            fbytes: get_value_from_file_attributes!(
                self,
                FileAttributeIdSpaceFree,
                FileAttributeSpaceFree,
                from_ref
            ),

            abytes: get_value_from_file_attributes!(
                self,
                FileAttributeIdSpaceAvail,
                FileAttributeSpaceAvail,
                from_ref
            ),

            tfiles: get_value_from_file_attributes!(
                self,
                FileAttributeIdFilesTotal,
                FileAttributeFilesTotal,
                from_ref
            ),

            ffiles: get_value_from_file_attributes!(
                self,
                FileAttributeIdFilesFree,
                FileAttributeFilesFree,
                from_ref
            ),

            afiles: get_value_from_file_attributes!(
                self,
                FileAttributeIdFilesAvail,
                FileAttributeFilesAvail,
                from_ref
            ),

            invarsec,

            attr: Some(self.into()),
        }
    }
}

impl From<&crate::Auth> for nfs4_client::OpaqueAuth {
    fn from(val: &crate::Auth) -> Self {
        match val.flavor {
            crate::AuthFlavor::Unix => {
                nfs4_client::OpaqueAuth::auth_sys(nfs4_client::AuthSysParameters {
                    stamp: 0,
                    machine_name: "nfs-rs".into(),
                    uid: nfs4_client::Uid(val.uid),
                    gid: nfs4_client::Gid(val.gid),
                    gids: Vec::new(),
                })
            }
        }
    }
}

impl From<crate::shared::Time> for nfs4::Time {
    fn from(val: crate::shared::Time) -> Self {
        nfs4::Time {
            seconds: val.seconds as i64,
            nseconds: val.nseconds,
        }
    }
}

impl From<nfs4::GetAttrRes> for crate::mount::Pathconf {
    fn from(val: nfs4::GetAttrRes) -> Self {
        val.object_attributes.into()
    }
}

impl From<nfs4::FileAttributes> for crate::mount::Pathconf {
    fn from(val: nfs4::FileAttributes) -> Self {
        crate::mount::Pathconf {
            linkmax: get_value_from_file_attributes!(
                val,
                FileAttributeIdMaxLink,
                FileAttributeMaxLink,
                from_ref
            ),
            name_max: get_value_from_file_attributes!(
                val,
                FileAttributeIdMaxName,
                FileAttributeMaxName,
                from_ref
            ),
            no_trunc: get_value_from_file_attributes!(
                val,
                FileAttributeIdNoTrunc,
                FileAttributeNoTrunc,
                from_ref
            ),
            chown_restricted: get_value_from_file_attributes!(
                val,
                FileAttributeIdChownRestricted,
                FileAttributeChownRestricted,
                from_ref
            ),
            case_insensitive: get_value_from_file_attributes!(
                val,
                FileAttributeIdCaseInsensitive,
                FileAttributeCaseInsensitive,
                from_ref
            ),
            case_preserving: get_value_from_file_attributes!(
                val,
                FileAttributeIdCasePreserving,
                FileAttributeCasePreserving,
                from_ref
            ),
            attr: Some(val.into()),
        }
    }
}

impl From<nfs4::GetAttrRes> for crate::mount::Attr {
    fn from(val: nfs4::GetAttrRes) -> Self {
        val.object_attributes.into()
    }
}

impl From<nfs4::FileAttributes> for crate::mount::Attr {
    fn from(val: nfs4::FileAttributes) -> Self {
        crate::mount::Attr {
            type_: get_value_from_file_attributes!(
                val,
                FileAttributeIdFileType,
                FileAttributeFileType,
                from_file_type
            ),
            file_mode: get_value_from_file_attributes!(
                val,
                FileAttributeIdFileMode,
                FileAttributeFileMode,
                from_file_mode
            ),
            nlink: get_value_from_file_attributes!(
                val,
                FileAttributeIdNumLinks,
                FileAttributeNumLinks,
                from_ref
            ),
            uid: get_value_from_file_attributes!(
                val,
                FileAttributeIdOwner,
                FileAttributeOwner,
                from_uid_or_gid_string
            ),
            gid: get_value_from_file_attributes!(
                val,
                FileAttributeIdOwnerGroup,
                FileAttributeOwnerGroup,
                from_uid_or_gid_string
            ),
            filesize: get_value_from_file_attributes!(
                val,
                FileAttributeIdFileSize,
                FileAttributeFileSize,
                from_ref
            ),
            used: get_value_from_file_attributes!(
                val,
                FileAttributeIdSpaceUsed,
                FileAttributeSpaceUsed,
                from_ref
            ),
            spec_data: get_value_from_file_attributes!(
                val,
                FileAttributeIdRawDev,
                FileAttributeRawDev,
                from_device_data
            ),
            fsid: get_value_from_file_attributes!(
                val,
                FileAttributeIdFsId,
                FileAttributeFsId,
                from_fs_id
            ),
            fileid: get_value_from_file_attributes!(
                val,
                FileAttributeIdFileId,
                FileAttributeFileId,
                from_file_id
            ),
            atime: get_value_from_file_attributes!(
                val,
                FileAttributeIdTimeAccess,
                FileAttributeTimeAccess,
                from_time
            ),
            mtime: get_value_from_file_attributes!(
                val,
                FileAttributeIdTimeModify,
                FileAttributeTimeModify,
                from_time
            ),
            ctime: get_value_from_file_attributes!(
                val,
                FileAttributeIdTimeCreate,
                FileAttributeTimeCreate,
                from_time
            ),
        }
    }
}

impl From<nfs4::DirectoryEntry> for crate::mount::ReaddirEntry {
    fn from(val: nfs4::DirectoryEntry) -> Self {
        let fileid = match val.attrs.get(FileAttributeIdFileId) {
            Some(FileAttributeFileId(value)) => from_file_id(value),
            _ => Default::default(),
        };
        crate::mount::ReaddirEntry {
            file_name: val.name.to_string(),
            fileid,
        }
    }
}

impl From<nfs4::DirectoryEntry> for crate::mount::ReaddirplusEntry {
    fn from(val: nfs4::DirectoryEntry) -> Self {
        let attrs = val.attrs;
        let handle = get_value_from_file_attributes!(
            attrs,
            FileAttributeIdFileHandle,
            FileAttributeFileHandle,
            from_file_handle
        );
        let attr: crate::mount::Attr = attrs.into();
        let fileid = attr.fileid;
        crate::mount::ReaddirplusEntry {
            fileid,
            file_name: val.name.to_string(),
            attr: Some(attr),
            handle,
        }
    }
}

fn into_error(err: nfs4_client::Error) -> Error {
    match err {
        // Direct mappings: NFSv4 StatusError -> NFSv3 NfsStatus
        // These have identical error codes in both protocols
        nfs4_client::Error::Protocol(nfs4::StatusError::Perm) => {
            Error::other(crate::shared::NfsStatus::Nfs3errPerm)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NoEnt) => {
            Error::other(crate::shared::NfsStatus::Nfs3errNoent)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Io) => {
            Error::other(crate::shared::NfsStatus::Nfs3errIo)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NxIo) => {
            Error::other(crate::shared::NfsStatus::Nfs3errNxio)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Access) => {
            Error::other(crate::shared::NfsStatus::Nfs3errAcces)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Exist) => {
            Error::other(crate::shared::NfsStatus::Nfs3errExist)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::XDev) => {
            Error::other(crate::shared::NfsStatus::Nfs3errXdev)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NotDir) => {
            Error::other(crate::shared::NfsStatus::Nfs3errNotdir)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Isdir) => {
            Error::other(crate::shared::NfsStatus::Nfs3errIsdir)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Inval) => {
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::FBig) => {
            Error::other(crate::shared::NfsStatus::Nfs3errFbig)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NoSpc) => {
            Error::other(crate::shared::NfsStatus::Nfs3errNospc)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::RoFs) => {
            Error::other(crate::shared::NfsStatus::Nfs3errRofs)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::MLink) => {
            Error::other(crate::shared::NfsStatus::Nfs3errMlink)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NameTooLong) => {
            Error::other(crate::shared::NfsStatus::Nfs3errNametoolong)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NotEmpty) => {
            Error::other(crate::shared::NfsStatus::Nfs3errNotempty)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::DQuot) => {
            Error::other(crate::shared::NfsStatus::Nfs3errDquot)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Stale) => {
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }

        // NFSv4-specific errors that map to NFSv3 equivalents with code >= 10001
        nfs4_client::Error::Protocol(nfs4::StatusError::BadHandle) => {
            Error::other(crate::shared::NfsStatus::Nfs3errBadhandle)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadCookie) => {
            Error::other(crate::shared::NfsStatus::Nfs3errBadCookie)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NotSupported) => {
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::TooSmall) => {
            Error::other(crate::shared::NfsStatus::Nfs3errToosmall)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::ServerFault) => {
            Error::other(crate::shared::NfsStatus::Nfs3errServerfault)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadType) => {
            Error::other(crate::shared::NfsStatus::Nfs3errBadtype)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Delay) => {
            Error::other(crate::shared::NfsStatus::Nfs3errJukebox)
        }

        // NFSv4-specific errors that map to closest NFSv3 equivalent
        // Map to existing NFSv3 errors based on semantic similarity
        nfs4_client::Error::Protocol(nfs4::StatusError::Denied) => {
            // Access denied -> map to access error
            Error::other(crate::shared::NfsStatus::Nfs3errAcces)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::Expired) => {
            // Expired handle -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::FhExpired) => {
            // File handle expired -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::StaleClientId) => {
            // Stale client ID -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::StaleStateId) => {
            // Stale state ID -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::OldStateId) => {
            // Old state ID -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::AdminRevoked) => {
            // Admin revoked -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::DeadSession) => {
            // Dead session -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::DelegRevoked) => {
            // Delegation revoked -> map to stale
            Error::other(crate::shared::NfsStatus::Nfs3errStale)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::ShareDenied) => {
            // Share denied -> map to access error
            Error::other(crate::shared::NfsStatus::Nfs3errAcces)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::WrongSec) => {
            // Wrong security -> map to access error
            Error::other(crate::shared::NfsStatus::Nfs3errAcces)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::OpenMode) => {
            // Wrong open mode -> map to access error
            Error::other(crate::shared::NfsStatus::Nfs3errAcces)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::WrongCred) => {
            // Wrong credentials -> map to access error
            Error::other(crate::shared::NfsStatus::Nfs3errAcces)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::RejectDeleg) => {
            // Reject delegation -> map to access error
            Error::other(crate::shared::NfsStatus::Nfs3errAcces)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::ClidInUse) => {
            // Client ID in use -> map to exist
            Error::other(crate::shared::NfsStatus::Nfs3errExist)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::ReclaimConflict) => {
            // Reclaim conflict -> map to exist
            Error::other(crate::shared::NfsStatus::Nfs3errExist)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::Moved) => {
            // File moved -> map to remote
            Error::other(crate::shared::NfsStatus::Nfs3errRemote)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::LeaseMoved) => {
            // Lease moved -> map to remote
            Error::other(crate::shared::NfsStatus::Nfs3errRemote)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::BadStateId) => {
            // Bad state ID -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadSeqId) => {
            // Bad sequence ID -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NotSame) => {
            // Not same -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::LockRange) => {
            // Lock range -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::RestoreFh) => {
            // Restore FH error -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::ReclaimBad) => {
            // Bad reclaim -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadXdr) => {
            // Bad XDR -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadOwner) => {
            // Bad owner -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadChar) => {
            // Bad character -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadName) => {
            // Bad name -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadIoMode) => {
            // Bad IO mode -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadLayout) => {
            // Bad layout -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadSessionDigest) => {
            // Bad session digest -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadSession) => {
            // Bad session -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadSlot) => {
            // Bad slot -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::ConnNotBoundToSession) => {
            // Connection not bound -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::SeqMisordered) => {
            // Sequence misordered -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::SequencePos) => {
            // Sequence position -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::UnsafeCompound) => {
            // Unsafe compound -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::OpNotInSession) => {
            // Op not in session -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::SeqFalseRetry) => {
            // Sequence false retry -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::BadHighSlot) => {
            // Bad high slot -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::WrongType) => {
            // Wrong type -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NotOnlyOp) => {
            // Not only op -> map to inval
            Error::other(crate::shared::NfsStatus::Nfs3errInval)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::AttrNotSupported) => {
            // Attribute not supported -> map to notsupp
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::LockNotSupported) => {
            // Lock not supported -> map to notsupp
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::OpIllegal) => {
            // Illegal operation -> map to notsupp
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::UnknownLayoutType) => {
            // Unknown layout type -> map to notsupp
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::HashAlgUnsupported) => {
            // Hash algorithm unsupported -> map to notsupp
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::EncrAlgUnsupported) => {
            // Encryption algorithm unsupported -> map to notsupp
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::DirDelegUnavail) => {
            // Directory delegation unavailable -> map to notsupp
            Error::other(crate::shared::NfsStatus::Nfs3errNotsupp)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::NoFileHandle) => {
            // No file handle -> map to badhandle
            Error::other(crate::shared::NfsStatus::Nfs3errBadhandle)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::Grace) => {
            // Grace period -> map to jukebox
            Error::other(crate::shared::NfsStatus::Nfs3errJukebox)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::NoGrace) => {
            // No grace -> map to jukebox
            Error::other(crate::shared::NfsStatus::Nfs3errJukebox)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::LayoutTryLater) => {
            // Layout try later -> map to jukebox
            Error::other(crate::shared::NfsStatus::Nfs3errJukebox)
        }
        nfs4_client::Error::Protocol(nfs4::StatusError::RetryUncachedRep) => {
            // Retry uncached -> map to jukebox
            Error::other(crate::shared::NfsStatus::Nfs3errJukebox)
        }

        nfs4_client::Error::Protocol(nfs4::StatusError::BadRange) => {
            // Bad range -> could map to fbig or inval, using fbig
            Error::other(crate::shared::NfsStatus::Nfs3errFbig)
        }

        // NFSv4-specific errors that don't have good NFSv3 equivalents
        // Map to IO error as a generic fallback
        nfs4_client::Error::Protocol(nfs4::StatusError::Same)
        | nfs4_client::Error::Protocol(nfs4::StatusError::Locked)
        | nfs4_client::Error::Protocol(nfs4::StatusError::MinorVersMismatch)
        | nfs4_client::Error::Protocol(nfs4::StatusError::Symlink)
        | nfs4_client::Error::Protocol(nfs4::StatusError::LocksHeld)
        | nfs4_client::Error::Protocol(nfs4::StatusError::Deadlock)
        | nfs4_client::Error::Protocol(nfs4::StatusError::FileOpen)
        | nfs4_client::Error::Protocol(nfs4::StatusError::CbPathDown)
        | nfs4_client::Error::Protocol(nfs4::StatusError::CompleteAlready)
        | nfs4_client::Error::Protocol(nfs4::StatusError::DelegAlreadyWanted)
        | nfs4_client::Error::Protocol(nfs4::StatusError::BackChanBusy)
        | nfs4_client::Error::Protocol(nfs4::StatusError::LayoutUnavailable)
        | nfs4_client::Error::Protocol(nfs4::StatusError::NoMatchingLayout)
        | nfs4_client::Error::Protocol(nfs4::StatusError::RecallConflict)
        | nfs4_client::Error::Protocol(nfs4::StatusError::ReqTooBig)
        | nfs4_client::Error::Protocol(nfs4::StatusError::RepTooBig)
        | nfs4_client::Error::Protocol(nfs4::StatusError::RepTooBigToCache)
        | nfs4_client::Error::Protocol(nfs4::StatusError::TooManyOps)
        | nfs4_client::Error::Protocol(nfs4::StatusError::ClientIdBusy)
        | nfs4_client::Error::Protocol(nfs4::StatusError::PnfsIoHole)
        | nfs4_client::Error::Protocol(nfs4::StatusError::PnfsNoLayout)
        | nfs4_client::Error::Protocol(nfs4::StatusError::ReturnConflict) => {
            Error::other(crate::shared::NfsStatus::Nfs3errIo)
        }

        // Pass through IO errors
        nfs4_client::Error::Io(io_err) => io_err,

        // Catch-all for any other errors
        _ => Error::other(format!("received error: {err:?}")),
    }
}

fn from_ref<T: Clone>(value: &T) -> T {
    value.clone()
}

fn from_file_handle(file_handle: &nfs4::FileHandle) -> Vec<u8> {
    file_handle.0.to_vec()
}

fn from_file_type(file_type: &nfs4::FileType) -> u32 {
    match file_type {
        nfs4::FileType::Regular => 1,
        nfs4::FileType::Directory => 2,
        nfs4::FileType::Block => 3,
        nfs4::FileType::Character => 4,
        nfs4::FileType::Link => 5,
        nfs4::FileType::Socket => 6,
        nfs4::FileType::Fifo => 7,
        nfs4::FileType::AttrDir => 8,
    }
}

fn from_file_mode(file_mode: &nfs4::Mode) -> u32 {
    file_mode.0
}

fn from_uid_or_gid_string(uid_or_gid: &str) -> u32 {
    uid_or_gid.parse().unwrap_or_default()
}

fn from_device_data(dev_data: &nfs4::DeviceData) -> [u32; 2] {
    [dev_data.major, dev_data.minor]
}

fn from_fs_id(fsid: &nfs4::FsId) -> u64 {
    // FIXME: change type of crate::mount::Attr::fsid to [u64; 2]? (and just set to [fsid, 0] for NFSv3?)
    //        as is, below is incorrect and may cause two different file systems to be recognised as the
    //        same file system (though unlikely? - so perhaps this is the best we can do for now?)
    fsid.major | fsid.minor
}

fn from_file_id(fileid: &nfs4::FileId) -> u64 {
    fileid.0
}

fn from_time(time: &nfs4::Time) -> crate::shared::Time {
    crate::shared::Time {
        seconds: time.seconds as u32,
        nseconds: time.nseconds,
    }
}

fn from_read_link_res(read_link_res: nfs4::ReadLinkRes) -> String {
    read_link_res.link
}

fn from_read_res(read_res: nfs4::ReadRes) -> Vec<u8> {
    read_res.data
}

fn from_write_res(write_res: nfs4::WriteRes) -> u32 {
    write_res.count
}

pub(super) struct Mount4p1 {
    client: Arc<RwLock<nfs4_client::Client<TcpStream>>>,
    root_path: PathBuf,
    root_fh: Vec<u8>,
    rsize: u32,
    wsize: u32,
}

impl Mount4p1 {
    fn get_dir_fh_and_entry_name(&self, path: &str) -> Result<(Vec<u8>, String)> {
        let (dir_path, name) = split_path(path)?;
        self._lookup_path(&dir_path).map(|fh| (fh, name))
    }

    fn get_obj_res_with_attrs(
        client: &mut nfs4_client::Client<TcpStream>,
        fh: &[u8],
    ) -> Result<ObjRes> {
        let attr = Self::_getattr(client, fh)?;
        Ok(ObjRes {
            attr: Some(attr),
            fh: fh.to_vec(),
        })
    }

    fn _getattr(
        client: &mut nfs4_client::Client<TcpStream>,
        fh: &[u8],
    ) -> Result<crate::mount::Attr> {
        let handle = nfs4::FileHandle(fh.to_vec());
        client.get_attr(handle).map(Into::into).map_err(into_error)
    }

    fn _lookup(
        client: &mut nfs4_client::Client<TcpStream>,
        dir_fh: &[u8],
        entry_name: &str,
    ) -> Result<Vec<u8>> {
        let cleaned = path_clean::clean(entry_name);
        let dir_handle = nfs4::FileHandle(dir_fh.to_vec());
        client
            .look_up_in(dir_handle, cleaned)
            .map(|handle| handle.0)
            .map_err(into_error)
    }

    fn _lookup_path(&self, path: &str) -> Result<Vec<u8>> {
        let cleaned = path_clean::clean(path);
        if let Some(cleaned_str) = cleaned.to_str() {
            if cleaned_str.is_empty() || cleaned_str == ROOT_PATH {
                return Ok(self.root_fh.to_vec());
            }
        }
        let file_path = if cleaned.is_absolute() {
            cleaned.strip_prefix("/").unwrap()
        } else {
            cleaned.as_path()
        };
        let mut client = self.client.write().unwrap();
        client
            .look_up(self.root_path.join(file_path))
            .map(|handle| handle.0)
            .map_err(into_error)
    }
}

impl core::fmt::Debug for Mount4p1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Mount4p1")
            .field("root_path", &self.root_path)
            .field("root_fh", &self.root_fh)
            .field("rsize", &self.rsize)
            .field("wsize", &self.wsize)
            .finish()
    }
}

impl crate::Mount for Mount4p1 {
    fn get_max_read_size(&self) -> u32 {
        self.rsize
    }

    fn get_max_write_size(&self) -> u32 {
        self.wsize
    }

    fn fsinfo(&self) -> Result<crate::mount::FSInfo> {
        let mut client = self.client.write().unwrap();

        client
            .get_attr(nfs4::FileHandle(self.root_fh.to_vec()))
            .map(Into::into)
            .map_err(into_error)
    }

    fn fsstat(&self) -> Result<crate::mount::FSStat> {
        let mut client = self.client.write().unwrap();

        client
            .get_attr(nfs4::FileHandle(self.root_fh.to_vec()))
            .map(Into::into)
            .map_err(into_error)
    }

    fn null(&self) -> Result<()> {
        let mut client = self.client.write().unwrap();
        client.null().map_err(into_error)
    }

    fn access(&self, fh: &[u8], mode: u32) -> Result<u32> {
        let mut client = self.client.write().unwrap();
        let handle = nfs4::FileHandle(fh.to_vec());
        client
            .access(handle, mode)
            .map(|res| res.access.bits())
            .map_err(into_error)
    }

    fn access_path(&self, path: &str, mode: u32) -> Result<u32> {
        self.access(&self._lookup_path(path)?, mode)
    }

    fn close(&self, _seqid: u32, _stateid: u64) -> Result<()> {
        unimplemented!("close")
    }

    fn commit(&self, fh: &[u8], offset: u64, count: u32) -> Result<()> {
        let mut client = self.client.write().unwrap();
        let handle = nfs4::FileHandle(fh.to_vec());
        client
            .commit(handle, offset, count)
            .map(|_commit_res| ()) // FIXME: change trait to return verifier (u64?) from commit function?
            .map_err(into_error)
    }

    fn commit_path(&self, path: &str, offset: u64, count: u32) -> Result<()> {
        self.commit(&self._lookup_path(path)?, offset, count)
    }

    fn create(&self, dir_fh: &[u8], filename: &str, mode: u32) -> Result<ObjRes> {
        let mut client = self.client.write().unwrap();
        let dir_handle = nfs4::FileHandle(dir_fh.to_vec());
        let now = std::time::SystemTime::now();
        let since_epoch = now.duration_since(std::time::UNIX_EPOCH).unwrap();
        let mtime = nfs4::Time {
            seconds: since_epoch.as_secs() as i64,
            nseconds: since_epoch.subsec_nanos(),
        };
        let mut attrs = FileAttributes::default();
        attrs.insert(FileAttributeFileMode(FileMode(mode)));
        attrs.insert(FileAttributeTimeModifySet(FileSetTime::SetToClientTime(
            mtime,
        )));
        let handle = client
            .create_file(dir_handle, filename, attrs)
            .map_err(into_error)?;
        Self::get_obj_res_with_attrs(&mut client, &handle.0)
    }

    fn create_path(&self, path: &str, mode: u32) -> Result<ObjRes> {
        let (dir_fh, filename) = self.get_dir_fh_and_entry_name(path)?;
        self.create(&dir_fh, &filename, mode)
    }

    fn delegpurge(&self, _clientid: u64) -> Result<()> {
        unimplemented!("delegpurge")
    }

    fn delegreturn(&self, _stateid: u64) -> Result<()> {
        unimplemented!("delegreturn")
    }

    fn getattr(&self, fh: &[u8]) -> Result<crate::mount::Attr> {
        let mut client = self.client.write().unwrap();
        Self::_getattr(&mut client, fh)
    }

    fn getattr_path(&self, path: &str) -> Result<crate::mount::Attr> {
        self.getattr(&self._lookup_path(path)?)
    }

    fn setattr(
        &self,
        fh: &[u8],
        guard_mtime: Option<Time>,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<Time>,
        mtime: Option<Time>,
    ) -> Result<()> {
        let mut client = self.client.write().unwrap();
        let handle = nfs4::FileHandle(fh.to_vec());
        let opts: Vec<nfs4::FileAttribute> = vec![
            mode.map(|m| FileAttributeFileMode(FileMode(m))),
            uid.map(|u| FileAttributeOwner(u.to_string())),
            gid.map(|g| FileAttributeOwnerGroup(g.to_string())),
            size.map(FileAttributeFileSize),
            atime.map(|a| FileAttributeTimeAccessSet(FileSetTime::SetToClientTime(a.into()))),
            mtime.map(|m| FileAttributeTimeModifySet(FileSetTime::SetToClientTime(m.into()))),
        ]
        .into_iter()
        .flatten()
        .collect();
        let attrs = FileAttributes::from_iter(opts);
        if let Some(guard_mtime) = guard_mtime {
            let verif_attrs = vec![FileAttributeTimeModify(guard_mtime.into())];
            client
                .set_attr_verified(handle, attrs, FileAttributes::from_iter(verif_attrs))
                .map_err(into_error)
        } else {
            client.set_attr(handle, attrs).map_err(into_error)
        }
    }

    fn setattr_path(
        &self,
        path: &str,
        specify_guard: bool,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<Time>,
        mtime: Option<Time>,
    ) -> Result<()> {
        let obj_res = self.lookup_path(path)?;
        let guard_mtime = match (specify_guard, obj_res.attr) {
            (true, Some(attr)) => Some(attr.mtime),
            (true, None) => self.getattr_path(path).map(|attr| Some(attr.mtime))?,
            _ => None,
        };
        self.setattr(&obj_res.fh, guard_mtime, mode, uid, gid, size, atime, mtime)
    }

    fn getfh(&self) -> Result<()> {
        unimplemented!("getfh")
    }

    fn link(
        &self,
        src_fh: &[u8],
        dst_dir_fh: &[u8],
        dst_filename: &str,
    ) -> Result<crate::mount::Attr> {
        let mut client = self.client.write().unwrap();
        let src_handle = nfs4::FileHandle(src_fh.to_vec());
        let dst_dir_handle = nfs4::FileHandle(dst_dir_fh.to_vec());
        let _ = client
            .link(src_handle, dst_dir_handle, dst_filename)
            .map_err(into_error)?;
        let fh = Self::_lookup(&mut client, dst_dir_fh, dst_filename)?;
        Self::_getattr(&mut client, &fh)
    }

    fn link_path(&self, src_path: &str, dst_path: &str) -> Result<crate::mount::Attr> {
        let (dst_dir_fh, dst_filename) = self.get_dir_fh_and_entry_name(dst_path)?;
        self.link(&self._lookup_path(src_path)?, &dst_dir_fh, &dst_filename)
    }

    fn symlink(&self, src_path: &str, dst_dir_fh: &[u8], dst_filename: &str) -> Result<ObjRes> {
        let mut client = self.client.write().unwrap();
        let dst_dir_handle = nfs4::FileHandle(dst_dir_fh.to_vec());
        let mut attrs = FileAttributes::default();
        attrs.insert(FileAttributeFileMode(FileMode(0o777)));
        // XXX: unlike `client.create_file`, `client.create_link` does not need to have set modify time attr
        let handle = client
            .create_link(src_path, dst_dir_handle, dst_filename, attrs)
            .map_err(into_error)?;
        Self::get_obj_res_with_attrs(&mut client, &handle.0)
    }

    fn symlink_path(&self, src_path: &str, dst_path: &str) -> Result<ObjRes> {
        let (dst_dir_fh, dst_filename) = self.get_dir_fh_and_entry_name(dst_path)?;
        self.symlink(src_path, &dst_dir_fh, &dst_filename)
    }

    fn readlink(&self, fh: &[u8]) -> Result<String> {
        let mut client = self.client.write().unwrap();
        let handle = nfs4::FileHandle(fh.to_vec());
        client
            .read_link(handle)
            .map(from_read_link_res)
            .map_err(into_error)
    }

    fn readlink_path(&self, path: &str) -> Result<String> {
        self.readlink(&self._lookup_path(path)?)
    }

    fn lookup(&self, dir_fh: &[u8], filename: &str) -> Result<ObjRes> {
        let mut client = self.client.write().unwrap();
        let fh = Self::_lookup(&mut client, dir_fh, filename)?;
        Self::get_obj_res_with_attrs(&mut client, &fh)
    }

    fn lookup_path(&self, path: &str) -> Result<ObjRes> {
        let fh = self._lookup_path(path)?;
        let mut client = self.client.write().unwrap();
        Self::get_obj_res_with_attrs(&mut client, &fh)
    }

    fn pathconf(&self, fh: &[u8]) -> Result<crate::mount::Pathconf> {
        let mut client = self.client.write().unwrap();
        let handle = nfs4::FileHandle(fh.to_vec());
        client.get_attr(handle).map(Into::into).map_err(into_error)
    }

    fn pathconf_path(&self, path: &str) -> Result<crate::mount::Pathconf> {
        self.pathconf(&self._lookup_path(path)?)
    }

    fn read(&self, fh: &[u8], offset: u64, count: u32) -> Result<Vec<u8>> {
        let mut client = self.client.write().unwrap();
        let handle = nfs4::FileHandle(fh.to_vec());
        client
            .read(handle, offset, count)
            .map(from_read_res)
            .map_err(into_error)
    }

    fn read_path(&self, path: &str, offset: u64, count: u32) -> Result<Vec<u8>> {
        self.read(&self._lookup_path(path)?, offset, count)
    }

    fn write(&self, fh: &[u8], offset: u64, data: &[u8]) -> Result<u32> {
        let mut client = self.client.write().unwrap();
        let max_write_size = self.get_max_write_size();
        let data_len = data.len();
        let mut index = 0usize;
        let mut offset = offset;
        let mut remaining = data_len as u32;
        while remaining > 0 {
            let chunk_size = remaining.min(max_write_size) as usize;
            let end_index = index + chunk_size;
            let chunk = &data[index..end_index];
            let handle = nfs4::FileHandle(fh.to_vec());
            let write_size = client
                .write(handle, offset, chunk.to_vec())
                .map(from_write_res)
                .map_err(into_error)?;
            remaining -= write_size;
            offset += write_size as u64;
            index += write_size as usize;
        }
        Ok(data_len as u32)
    }

    fn write_path(&self, path: &str, offset: u64, data: &[u8]) -> Result<u32> {
        self.write(&self._lookup_path(path)?, offset, data)
    }

    fn readdir(&self, dir_fh: &[u8]) -> Result<Vec<crate::mount::ReaddirEntry>> {
        let mut client = self.client.write().unwrap();
        let dir_handle = nfs4::FileHandle(dir_fh.to_vec());
        let attr_ids = vec![FileAttributeIdFileId];
        let attrs = nfs4::EnumSet::<nfs4::FileAttributeId>::from_iter(attr_ids);
        client
            .read_dir(dir_handle, attrs)
            .map(|entries| entries.into_iter().map(Into::into).collect())
            .map_err(into_error)
    }

    fn readdir_path(&self, dir_path: &str) -> Result<Vec<crate::mount::ReaddirEntry>> {
        self.readdir(&self._lookup_path(dir_path)?)
    }

    fn readdirplus(&self, dir_fh: &[u8]) -> Result<Vec<crate::mount::ReaddirplusEntry>> {
        let mut client = self.client.write().unwrap();
        let dir_handle = nfs4::FileHandle(dir_fh.to_vec());
        let attr_ids = vec![
            FileAttributeIdFileType,
            FileAttributeIdFileMode,
            FileAttributeIdNumLinks,
            FileAttributeIdOwner,
            FileAttributeIdOwnerGroup,
            FileAttributeIdFileSize,
            FileAttributeIdSpaceUsed,
            FileAttributeIdRawDev,
            FileAttributeIdFsId,
            FileAttributeIdFileId,
            FileAttributeIdFileHandle,
            FileAttributeIdTimeAccess,
            FileAttributeIdTimeModify,
            FileAttributeIdTimeCreate,
        ];
        let attrs = nfs4::EnumSet::<nfs4::FileAttributeId>::from_iter(attr_ids);
        client
            .read_dir(dir_handle, attrs)
            .map(|entries| entries.into_iter().map(Into::into).collect())
            .map_err(into_error)
    }

    fn readdirplus_path(&self, dir_path: &str) -> Result<Vec<crate::mount::ReaddirplusEntry>> {
        self.readdirplus(&self._lookup_path(dir_path)?)
    }

    fn mkdir(&self, dir_fh: &[u8], dirname: &str, mode: u32) -> Result<ObjRes> {
        let mut client = self.client.write().unwrap();
        let dir_handle = nfs4::FileHandle(dir_fh.to_vec());
        let mut attrs = FileAttributes::default();
        attrs.insert(FileAttributeFileMode(FileMode(mode)));
        // XXX: unlike `client.create_file`, `client.create_directory` does not need to have set modify time attr
        let handle = client
            .create_directory(dir_handle, dirname, attrs)
            .map_err(into_error)?;
        Self::get_obj_res_with_attrs(&mut client, &handle.0)
    }

    fn mkdir_path(&self, path: &str, mode: u32) -> Result<ObjRes> {
        let (dir_fh, dirname) = self.get_dir_fh_and_entry_name(path)?;
        self.mkdir(&dir_fh, &dirname, mode)
    }

    fn remove(&self, dir_fh: &[u8], filename: &str) -> Result<()> {
        let mut client = self.client.write().unwrap();
        let dir_handle = nfs4::FileHandle(dir_fh.to_vec());
        client
            .remove(dir_handle, filename)
            .map(|_change_info| ()) // FIXME: change trait to return change info from remove function? (what would we do for NFSv3?)
            .map_err(into_error)
    }

    fn remove_path(&self, path: &str) -> Result<()> {
        let (dir_fh, filename) = self.get_dir_fh_and_entry_name(path)?;
        self.remove(&dir_fh, &filename)
    }

    fn rmdir(&self, dir_fh: &[u8], dirname: &str) -> Result<()> {
        self.remove(dir_fh, dirname) // XXX: NFSv4.1 has no rmdir operation, just remove (which works for both files and directories)
    }

    fn rmdir_path(&self, path: &str) -> Result<()> {
        let (dir_fh, dirname) = self.get_dir_fh_and_entry_name(path)?;
        self.rmdir(&dir_fh, &dirname)
    }

    fn rename(
        &self,
        from_dir_fh: &[u8],
        from_filename: &str,
        to_dir_fh: &[u8],
        to_filename: &str,
    ) -> Result<()> {
        let mut client = self.client.write().unwrap();
        let from_dir_handle = nfs4::FileHandle(from_dir_fh.to_vec());
        let to_dir_handle = nfs4::FileHandle(to_dir_fh.to_vec());
        client
            .rename(from_dir_handle, to_dir_handle, from_filename, to_filename)
            .map(|_rename_res| ()) // FIXME: change trait to return rename res (two change infos) from rename function? (what would we do for NFSv3?)
            .map_err(into_error)
    }

    fn rename_path(&self, from_path: &str, to_path: &str) -> Result<()> {
        let (from_dir_fh, from_filename) = self.get_dir_fh_and_entry_name(from_path)?;
        let (to_dir_fh, to_filename) = self.get_dir_fh_and_entry_name(to_path)?;
        self.rename(&from_dir_fh, &from_filename, &to_dir_fh, &to_filename)
    }

    fn umount(&self) -> Result<()> {
        unimplemented!("umount")
    }

    fn version(&self) -> NFSVersion {
        NFSVersion::NFSv4p1
    }
}

pub(crate) fn mount(args: &crate::MountArgs) -> Result<Box<dyn crate::Mount>> {
    // start by resolving host address and assigning portmapper port to each resolved address
    let addrs: Vec<SocketAddr> = (args.host.as_str(), args.port) // NFSv4 always uses port 2049
        .to_socket_addrs()?
        .collect();

    // Create a unique client identifier
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let unique_id = format!("nfs-rs-{:x}", nanos);

    let auth = crate::Auth::new_unix(&unique_id, args.uid, args.gid);

    for addr in addrs {
        let res = mount_on_addr(&addr, args, &auth);
        if let Ok(a) = res {
            return Ok(a);
        }
    }

    Err(io::Error::other("no valid socket address"))
}

fn mount_on_addr(
    addr: &SocketAddr,
    args: &crate::MountArgs,
    auth: &crate::Auth,
) -> Result<Box<dyn crate::Mount>> {
    let tcp_stream = TcpStream::connect(addr)?;
    let mut client = nfs4_client::Client::new(tcp_stream, Some(auth.into()))
        .map_err(|e| io::Error::other(format!("nfs4_client::Client::new returned error: {e:?}")))?;
    let root_path = path_clean::clean(&args.dirpath);
    let root_fh = client.look_up(&root_path).map_err(into_error)?;
    let rsize = args.rsize.min(client.get_max_read_size() as u32);
    let wsize = args.wsize.min(client.get_max_write_size() as u32);
    Ok(Box::new(Mount4p1 {
        client: Arc::new(RwLock::new(client)),
        root_path,
        root_fh: root_fh.0,
        rsize,
        wsize,
    }))
}
