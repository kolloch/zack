use std::collections::BTreeSet;

use nix::{
    errno::Errno,
    unistd::{Gid, Group, User},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NameAndId {
    pub name: Option<String>,
    pub id: u32,
}

impl NameAndId {
    pub fn current_user() -> Result<NameAndId, Errno> {
        let my_uid = nix::unistd::getuid();
        let my_user = User::from_uid(my_uid)?;
        let my_name = my_user.map(|u| u.name);
        Ok(NameAndId {
            name: my_name,
            id: my_uid.as_raw(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Groups {
    main: NameAndId,
    extra: BTreeSet<NameAndId>,
}

impl Groups {
    pub fn current() -> Result<Groups, Errno> {
        let my_gid = nix::unistd::getegid();

        let extra = nix::unistd::getgroups()?;

        fn name_and_id(gid: Gid) -> Result<NameAndId, Errno> {
            let group = Group::from_gid(gid)?;
            let name = group.map(|g| g.name);
            Ok(NameAndId {
                name,
                id: gid.as_raw(),
            })
        }

        Ok(Groups {
            main: name_and_id(my_gid)?,
            extra: extra
                .iter()
                .map(|gid| name_and_id(*gid))
                .collect::<Result<BTreeSet<_>, _>>()?,
        })
    }
}
