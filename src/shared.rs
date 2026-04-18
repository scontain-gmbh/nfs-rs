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

/// Struct describing an NFS timestamp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Time {
    pub seconds: u32,
    pub nseconds: u32,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NfsStatus {
    Nfs3Ok = 0isize,
    Nfs3errPerm = 1isize,
    Nfs3errNoent = 2isize,
    Nfs3errIo = 5isize,
    Nfs3errNxio = 6isize,
    Nfs3errAcces = 13isize,
    Nfs3errExist = 17isize,
    Nfs3errXdev = 18isize,
    Nfs3errNodev = 19isize,
    Nfs3errNotdir = 20isize,
    Nfs3errIsdir = 21isize,
    Nfs3errInval = 22isize,
    Nfs3errFbig = 27isize,
    Nfs3errNospc = 28isize,
    Nfs3errRofs = 30isize,
    Nfs3errMlink = 31isize,
    Nfs3errNametoolong = 63isize,
    Nfs3errNotempty = 66isize,
    Nfs3errDquot = 69isize,
    Nfs3errStale = 70isize,
    Nfs3errRemote = 71isize,
    Nfs3errBadhandle = 10001isize,
    Nfs3errNotSync = 10002isize,
    Nfs3errBadCookie = 10003isize,
    Nfs3errNotsupp = 10004isize,
    Nfs3errToosmall = 10005isize,
    Nfs3errServerfault = 10006isize,
    Nfs3errBadtype = 10007isize,
    Nfs3errJukebox = 10008isize,
}

impl std::error::Error for NfsStatus {}

impl std::fmt::Display for NfsStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NfsStatus::Nfs3Ok => write!(f, "call completed successfully"),
            NfsStatus::Nfs3errPerm => write!(f, "permission denied"),
            NfsStatus::Nfs3errNoent => write!(f, "no such file or directory"),
            NfsStatus::Nfs3errNxio => write!(f, "i/o error - no such device or address"),
            NfsStatus::Nfs3errAcces => write!(f, "access denied"),
            NfsStatus::Nfs3errExist => write!(f, "file exists"),
            NfsStatus::Nfs3errXdev => write!(f, "cross-device hard link not allowed"),
            NfsStatus::Nfs3errNodev => write!(f, "no such device"),
            NfsStatus::Nfs3errNotdir => write!(f, "not a directory"),
            NfsStatus::Nfs3errIsdir => write!(f, "is a directory"),
            NfsStatus::Nfs3errInval => write!(f, "invalid or unsupported argument"),
            NfsStatus::Nfs3errFbig => write!(f, "file too large"),
            NfsStatus::Nfs3errNospc => write!(f, "no space left on device"),
            NfsStatus::Nfs3errRofs => write!(f, "read-only file system"),
            NfsStatus::Nfs3errMlink => write!(f, "too many hard links"),
            NfsStatus::Nfs3errNametoolong => write!(f, "name is too long"),
            NfsStatus::Nfs3errNotempty => write!(f, "directory not empty"),
            NfsStatus::Nfs3errDquot => write!(f, "resource (quota) hard limit exceeded"),
            NfsStatus::Nfs3errStale => write!(f, "invalid file handle"),
            NfsStatus::Nfs3errRemote => write!(f, "too many levels of remote in path"),
            NfsStatus::Nfs3errBadhandle => write!(f, "illegal NFS file handle"),
            NfsStatus::Nfs3errNotSync => write!(f, "update synchronization mismatch"),
            NfsStatus::Nfs3errBadCookie => write!(f, "cookie is stale"),
            NfsStatus::Nfs3errNotsupp => write!(f, "operation is not supported"),
            NfsStatus::Nfs3errToosmall => write!(f, "buffer or request is too small"),
            NfsStatus::Nfs3errServerfault => write!(f, "internal server error"),
            NfsStatus::Nfs3errBadtype => write!(f, "type not supported by server"),
            NfsStatus::Nfs3errJukebox => write!(f, "try again"),
            NfsStatus::Nfs3errIo => write!(
                f,
                "i/o error occurred while processing the requested operation"
            ),
        }
    }
}
