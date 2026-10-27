use crate::store::CloudBase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrincipalView<'a> {
    pub id: &'a str,
    pub digest: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecideInput<'a> {
    pub local: Option<&'a str>,
    pub base: Option<&'a CloudBase>,
    pub principal: Option<PrincipalView<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Nothing,
    Adopt { snapshot: String },
    Push { parent: Option<String> },
    Pull { snapshot: String },
    Branch { parent: String },
    AdoptCloud { snapshot: String },
}

pub fn decide(input: &DecideInput) -> Decision {
    let Some(principal) = input.principal else {
        return match input.local {
            None => Decision::Nothing,
            Some(_) => Decision::Push {
                parent: input.base.map(|b| b.snapshot_id.clone()),
            },
        };
    };
    let snapshot = principal.id.to_owned();
    let on_principal = input.base.is_some_and(|b| b.snapshot_id == principal.id);
    if input.local == Some(principal.digest) && !on_principal {
        return Decision::Adopt { snapshot };
    }
    let Some(base) = input.base else {
        return match input.local {
            None => Decision::Pull { snapshot },
            Some(_) => Decision::AdoptCloud { snapshot },
        };
    };
    let Some(local) = input.local else {
        return Decision::Pull { snapshot };
    };
    let local_changed = local != base.digest;
    match (local_changed, !on_principal) {
        (false, false) => Decision::Nothing,
        (true, false) => Decision::Push {
            parent: Some(base.snapshot_id.clone()),
        },
        (false, true) => Decision::Pull { snapshot },
        (true, true) => Decision::Branch {
            parent: base.snapshot_id.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(snapshot: &str, digest: &str) -> CloudBase {
        CloudBase {
            game_key: "7".into(),
            snapshot_id: snapshot.into(),
            digest: digest.into(),
            playtime_seconds: None,
            synced_at: 0,
            profile: String::new(),
        }
    }

    fn run(
        local: Option<&str>,
        base: Option<&CloudBase>,
        principal: Option<(&str, &str)>,
    ) -> Decision {
        decide(&DecideInput {
            local,
            base,
            principal: principal.map(|(id, digest)| PrincipalView { id, digest }),
        })
    }

    #[test]
    fn no_cloud_no_local_does_nothing() {
        assert_eq!(run(None, None, None), Decision::Nothing);
    }

    #[test]
    fn first_push_without_principal() {
        assert_eq!(run(Some("l"), None, None), Decision::Push { parent: None });
        let b = base("gone", "old");
        assert_eq!(
            run(Some("l"), Some(&b), None),
            Decision::Push {
                parent: Some("gone".into())
            }
        );
    }

    #[test]
    fn identical_local_adopts_without_copying() {
        assert_eq!(
            run(Some("d"), None, Some(("p", "d"))),
            Decision::Adopt {
                snapshot: "p".into()
            }
        );
        let b = base("old", "x");
        assert_eq!(
            run(Some("d"), Some(&b), Some(("p", "d"))),
            Decision::Adopt {
                snapshot: "p".into()
            }
        );
    }

    #[test]
    fn new_pc_without_save_pulls() {
        assert_eq!(
            run(None, None, Some(("p", "d"))),
            Decision::Pull {
                snapshot: "p".into()
            }
        );
    }

    #[test]
    fn new_pc_with_other_save_adopts_cloud() {
        assert_eq!(
            run(Some("l"), None, Some(("p", "d"))),
            Decision::AdoptCloud {
                snapshot: "p".into()
            }
        );
    }

    #[test]
    fn missing_local_save_is_restored() {
        let b = base("p", "d");
        assert_eq!(
            run(None, Some(&b), Some(("p", "d"))),
            Decision::Pull {
                snapshot: "p".into()
            }
        );
    }

    #[test]
    fn unchanged_both_sides_does_nothing() {
        let b = base("p", "d");
        assert_eq!(
            run(Some("d"), Some(&b), Some(("p", "d"))),
            Decision::Nothing
        );
    }

    #[test]
    fn only_local_changed_pushes() {
        let b = base("p", "d");
        assert_eq!(
            run(Some("l"), Some(&b), Some(("p", "d"))),
            Decision::Push {
                parent: Some("p".into())
            }
        );
    }

    #[test]
    fn only_cloud_changed_pulls() {
        let b = base("p", "d");
        assert_eq!(
            run(Some("d"), Some(&b), Some(("q", "e"))),
            Decision::Pull {
                snapshot: "q".into()
            }
        );
    }

    #[test]
    fn both_changed_branches() {
        let b = base("p", "d");
        assert_eq!(
            run(Some("l"), Some(&b), Some(("q", "e"))),
            Decision::Branch { parent: "p".into() }
        );
    }
}
