use crate::{
    error,
    invarch::{self, runtime_types::pallet_inv4::pallet::AnyId},
    util::generate_cid,
};
use cid::Cid;
use codec::{Decode, Encode};
use futures::TryStreamExt;
use git2::{Blob, Commit, Object, ObjectType, Odb, Oid, Repository, Tag, Tree};
use ipfs_api::{IpfsApi, IpfsClient};
use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    error::Error,
    io::Cursor,
};
use subxt::{sp_core::H256, DefaultConfig, PairSigner, PolkadotExtrinsicParams};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub chain_endpoint: String,
}

/// A magic value used to signal that a hash is a submodule tip (to be obtained by git on its own).
pub static SUBMODULE_TIP_MARKER: &str = "submodule-tip";

pub type BoxResult<T> = Result<T, Box<dyn Error>>;

#[derive(Encode, Decode, Debug)]
pub struct RefsFile {
    pub refs: Vec<(String, String)>,
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct GitObject {
    /// The git hash of the underlying git object
    pub git_hash: String,
    /// A link to the raw form of the object
    pub raw_data_ipfs_hash: Vec<u8>,
    /// Object-type-specific metadata
    pub metadata: GitObjectMetadata,
}

#[derive(Clone, Debug, Encode, Decode)]
pub enum GitObjectMetadata {
    #[allow(missing_docs)]
    Commit {
        parent_git_hashes: BTreeSet<String>,
        tree_git_hash: String,
    },
    #[allow(missing_docs)]
    Tag { target_git_hash: String },
    #[allow(missing_docs)]
    Tree { entry_git_hashes: BTreeSet<String> },
    #[allow(missing_docs)]
    Blob,
}

impl GitObject {
    pub async fn chain_get(
        git_hash: String,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        ips_id: u32,
    ) -> Result<Self, Box<dyn Error>> {
        let ips_info = chain_api
            .storage()
            .inv4()
            .ip_storage(&ips_id, None)
            .await?
            .ok_or(format!("IPS {ips_id} does not exist"))?;

        for file in ips_info.data.0 {
            if let AnyId::IpfId(id) = file {
                let ipf_info = chain_api
                    .storage()
                    .ipf()
                    .ipf_storage(&id, None)
                    .await?
                    .ok_or("Internal error: IPF listed from IPS does not exist")?;
                if String::from_utf8(ipf_info.metadata.0.clone())? == *git_hash {
                    return Ok(Self::decode(
                        &mut ipfs
                            .cat(&generate_cid(ipf_info.data.0.into())?.to_string())
                            .map_ok(|c| c.to_vec())
                            .try_concat()
                            .await?
                            .as_slice(),
                    )?);
                }
            }
        }
        error!("git_hash ipf not found")
    }

    pub fn from_git_blob(blob: &Blob, odb: &Odb) -> Result<Self, Box<dyn Error>> {
        let odb_obj = odb.read(blob.id())?;

        Ok(Self {
            git_hash: blob.id().to_string(),
            raw_data_ipfs_hash: odb_obj.data().to_vec(),
            metadata: GitObjectMetadata::Blob,
        })
    }

    pub fn from_git_commit(commit: &Commit, odb: &Odb) -> Result<Self, Box<dyn Error>> {
        let odb_obj = odb.read(commit.id())?;

        let parent_git_hashes: BTreeSet<String> = commit
            .parent_ids()
            .map(|parent_id| format!("{}", parent_id))
            .collect();

        let tree_git_hash = format!("{}", commit.tree()?.id());

        Ok(Self {
            git_hash: commit.id().to_string(),
            raw_data_ipfs_hash: odb_obj.data().to_vec(),
            metadata: GitObjectMetadata::Commit {
                parent_git_hashes,
                tree_git_hash,
            },
        })
    }

    pub fn from_git_tag(tag: &Tag, odb: &Odb) -> Result<Self, Box<dyn Error>> {
        let odb_obj = odb.read(tag.id())?;

        Ok(Self {
            git_hash: tag.id().to_string(),
            raw_data_ipfs_hash: odb_obj.data().to_vec(),
            metadata: GitObjectMetadata::Tag {
                target_git_hash: format!("{}", tag.target_id()),
            },
        })
    }

    pub fn from_git_tree(tree: &Tree, odb: &Odb) -> Result<Self, Box<dyn Error>> {
        let odb_obj = odb.read(tree.id())?;

        let entry_git_hashes: BTreeSet<String> =
            tree.iter().map(|entry| format!("{}", entry.id())).collect();

        Ok(Self {
            git_hash: tree.id().to_string(),
            raw_data_ipfs_hash: odb_obj.data().to_vec(),
            metadata: GitObjectMetadata::Tree { entry_git_hashes },
        })
    }

    /// Put `self` on IPFS and return the link.
    pub async fn chain_add(
        &self,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        signer: &PairSigner<DefaultConfig, sp_keyring::sr25519::sr25519::Pair>,
    ) -> Result<(String, u64), Box<dyn Error>> {
        let git_hash = self.git_hash.clone();

        debug!("Pushing object to IPFS");
        let ipfs_hash =
            &Cid::try_from(ipfs.add(Cursor::new(self.encode())).await?.hash)?.to_bytes()[2..];

        debug!("Sending object to the chain");
        let events = chain_api
            .tx()
            .ipf()
            .mint(
                self.git_hash.as_bytes().to_vec(),
                H256::from_slice(ipfs_hash),
            )?
            .sign_and_submit_then_watch_default(signer)
            .await?
            .wait_for_finalized_success()
            .await?;

        let ipf_id = events
            .find_first::<invarch::ipf::events::Minted>()?
            .unwrap()
            .1;

        Ok((git_hash, ipf_id))
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct RepoData {
    /// All refs this repository knows; a {name -> sha1} mapping
    pub refs: BTreeMap<String, String>,
    /// All objects this repository contains; a {sha1} vec
    pub objects: Vec<String>,
}

impl RepoData {
    pub async fn from_ipfs(ipfs_hash: H256, ipfs: &mut IpfsClient) -> Result<Self, Box<dyn Error>> {
        let refs_cid = generate_cid(ipfs_hash)?.to_string();
        let refs_content = ipfs
            .cat(&refs_cid)
            .map_ok(|c| c.to_vec())
            .try_concat()
            .await?;

        Ok(Self::decode(&mut refs_content.as_slice())?)
    }

    pub async fn push_ref_from_str(
        &mut self,
        ref_src: &str,
        ref_dst: &str,
        force: bool,
        repo: &mut Repository,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        signer: &PairSigner<DefaultConfig, sp_keyring::sr25519::sr25519::Pair>,
        ips_id: u32,
    ) -> Result<Vec<u64>, Box<dyn Error>> {
        // Deleting `ref_dst` was requested
        if ref_src.is_empty() {
            debug!("Removing ref {} from index", ref_dst);
            if self.refs.remove(ref_dst).is_none() {
                debug!(
                    "Nothing to delete, ref {} not part of the index ref set",
                    ref_dst
                );
                debug!("Available refs:\n{:#?}", self.refs);
            }
            return Ok(vec![]);
        }
        let reference = repo.find_reference(ref_src)?.resolve()?;

        // Differentiate between annotated tags and their commit representation
        let obj = reference
            .peel(ObjectType::Tag)
            .unwrap_or(reference.peel(ObjectType::Commit)?);

        debug!(
            "{:?} dereferenced to {:?} {}",
            reference.shorthand(),
            obj.kind(),
            obj.id()
        );

        if force {
            eprintln!("This push will be forced");
        } else {
            eprintln!("Checking for work ahead of us...");

            if let Some(dst_git_hash) = self.refs.get(ref_dst) {
                let mut missing_objects = HashSet::new();
                self.enumerate_for_fetch(
                    dst_git_hash.parse()?,
                    &mut missing_objects,
                    repo,
                    ipfs,
                    chain_api,
                    ips_id,
                )
                .await?;

                if !missing_objects.is_empty() {
                    eprintln!(
                        "There's {} objects in {} not present locally. Please fetch first or force-push.",
                        missing_objects.len(),
                        ref_dst
                        );

                    debug!("Missing objects:\n{:#?}", missing_objects);
                    return Err("There's objects in the index not present in the local repo - a pull is needed".into());
                }
            }
        }

        let mut objs_for_push = HashSet::new();
        let mut submodules_for_push = HashSet::new();

        self.enumerate_for_push(
            &obj.clone(),
            &mut objs_for_push,
            &mut submodules_for_push,
            repo,
        )?;

        let ipf_id_list = self
            .push_git_objects(&objs_for_push, repo, ipfs, chain_api, signer)
            .await?;

        // Add all submodule tips to the index
        for _ in submodules_for_push {
            self.objects.push(SUBMODULE_TIP_MARKER.to_string());
        }

        self.refs
            .insert(ref_dst.to_owned(), format!("{}", obj.id()));
        Ok(ipf_id_list)
    }

    pub fn enumerate_for_push(
        &self,
        obj: &Object,
        push_todo: &mut HashSet<Oid>,
        submodules: &mut HashSet<Oid>,
        repo: &Repository,
    ) -> Result<(), Box<dyn Error>> {
        // Object tree traversal state
        let mut stack = vec![obj.clone()];

        let mut obj_cnt = 1;
        while let Some(obj) = stack.pop() {
            if self.objects.contains(&obj.id().to_string()) {
                debug!("Object {} already in RepoData", obj.id());
                continue;
            }

            if push_todo.contains(&obj.id()) {
                debug!("Object {} already in state", obj.id());
                continue;
            }

            let obj_type = obj.kind().ok_or_else(|| {
                let msg = format!("Cannot determine type of object {}", obj.id());
                debug!("{}", msg);
                msg
            })?;

            push_todo.insert(obj.id());

            match obj_type {
                ObjectType::Commit => {
                    let commit = obj
                        .as_commit()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a commit", obj))
                        .unwrap();
                    debug!("[{}] Counting commit {:?}", obj_cnt, commit);

                    let tree_obj = obj.peel(ObjectType::Tree)?;
                    debug!("Commit {}: Handling tree {}", commit.id(), tree_obj.id());

                    stack.push(tree_obj);

                    for parent in commit.parents() {
                        debug!(
                            "Commit {}: Pushing parent commit {}",
                            commit.id(),
                            parent.id()
                        );
                        stack.push(parent.into_object());
                    }
                }
                ObjectType::Tree => {
                    let tree = obj
                        .as_tree()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a tree", obj))
                        .unwrap();
                    debug!("[{}] Counting tree {:?}", obj_cnt, tree);

                    for entry in tree.into_iter() {
                        // Weed out submodules (Implicitly known as commit children of tree objects)
                        if let Some(ObjectType::Commit) = entry.kind() {
                            debug!("Skipping submodule at {}", entry.id());

                            submodules.insert(entry.id());

                            continue;
                        }

                        debug!(
                            "Tree {}: Pushing tree entry {} ({:?})",
                            tree.id(),
                            entry.id(),
                            entry.kind()
                        );

                        stack.push(entry.to_object(repo)?);
                    }
                }
                ObjectType::Blob => {
                    let blob = obj
                        .as_blob()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a blob", obj))
                        .unwrap();
                    debug!("[{}] Counting blob {:?}", obj_cnt, blob);
                }
                ObjectType::Tag => {
                    let tag = obj
                        .as_tag()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a tag", obj))
                        .unwrap();
                    debug!("[{}] Counting tag {:?}", obj_cnt, tag);

                    stack.push(tag.target()?);
                }
                other => {
                    return Err(format!("Don't know how to traverse a {}", other).into());
                }
            }

            obj_cnt += 1;
        }
        Ok(())
    }

    pub async fn fetch_to_ref_from_str(
        &self,
        git_hash: &str,
        ref_name: &str,
        repo: &mut Repository,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        ips_id: u32,
    ) -> Result<(), Box<dyn Error>> {
        debug!("Fetching {} for {}", git_hash, ref_name);

        let git_hash_oid = Oid::from_str(git_hash)?;
        let mut oids_for_fetch = HashSet::new();

        self.enumerate_for_fetch(
            git_hash_oid,
            &mut oids_for_fetch,
            repo,
            ipfs,
            chain_api,
            ips_id,
        )
        .await?;

        self.fetch_git_objects(&oids_for_fetch, repo, ipfs, chain_api, ips_id)
            .await?;

        match repo.odb()?.read_header(git_hash_oid)?.1 {
            ObjectType::Commit if ref_name.starts_with("refs/tags") => {
                debug!("Not setting ref for lightweight tag {}", ref_name);
            }
            ObjectType::Commit => {
                repo.reference(ref_name, git_hash_oid, true, "inv4-git fetch")?;
            }
            // Somehow git is upset when we set tag refs for it
            ObjectType::Tag => {
                debug!("Not setting ref for tag {}", ref_name);
            }
            other_type => {
                let msg = format!("New tip turned out to be a {} after fetch", other_type);
                debug!("{}", msg);
                return Err(msg.into());
            }
        }

        debug!("Fetched {} for {} OK.", git_hash, ref_name);
        Ok(())
    }

    pub async fn enumerate_for_fetch(
        &self,
        oid: Oid,
        fetch_todo: &mut HashSet<Oid>,
        repo: &Repository,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        ips_id: u32,
    ) -> Result<(), Box<dyn Error>> {
        let mut stack = vec![oid];

        while let Some(oid) = stack.pop() {
            if repo.odb()?.read_header(oid).is_ok() {
                debug!("Object {} already present locally!", oid);
                continue;
            }

            if fetch_todo.contains(&oid) {
                debug!("Object {} already present in state!", oid);
                continue;
            }

            let obj_git_hash = self
                .objects
                .iter()
                .find(|s| *s == &format!("{}", oid))
                .ok_or_else(|| {
                    let msg = format!("Could not find object {} in the index", oid);
                    debug!("{}", msg);
                    msg
                })?
                .clone();

            if obj_git_hash == SUBMODULE_TIP_MARKER {
                debug!("Ommitting submodule {}", oid.to_string());
                return Ok(());
            }

            fetch_todo.insert(oid);

            let git_obj =
                GitObject::chain_get(obj_git_hash.clone(), ipfs, chain_api, ips_id).await?;

            match git_obj.clone().metadata {
                GitObjectMetadata::Commit {
                    parent_git_hashes,
                    tree_git_hash,
                } => {
                    stack.push(Oid::from_str(&tree_git_hash)?);

                    for parent_git_hash in parent_git_hashes {
                        stack.push(Oid::from_str(&parent_git_hash)?);
                    }
                }
                GitObjectMetadata::Tag { target_git_hash } => {
                    stack.push(Oid::from_str(&target_git_hash)?);
                }
                GitObjectMetadata::Tree { entry_git_hashes } => {
                    for entry_git_hash in entry_git_hashes {
                        stack.push(Oid::from_str(&entry_git_hash)?);
                    }
                }
                GitObjectMetadata::Blob => {}
            }
        }

        Ok(())
    }

    pub async fn push_git_objects(
        &mut self,
        oids: &HashSet<Oid>,
        repo: &Repository,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        signer: &PairSigner<DefaultConfig, sp_keyring::sr25519::sr25519::Pair>,
    ) -> Result<Vec<u64>, Box<dyn Error>> {
        let mut ipf_id_list = vec![];

        let oid_count = oids.len();

        eprintln!("Minting {} IPFs", oid_count);

        for (i, oid) in oids.iter().enumerate() {
            let obj = repo.find_object(*oid, None)?;
            debug!("Current object: {:?} at {}", obj.kind(), obj.id());

            if self.objects.contains(&obj.id().to_string()) {
                debug!("push_objects: Object {} already in RepoData", obj.id());
                continue;
            }

            let obj_type = obj.kind().ok_or_else(|| {
                let msg = format!("Cannot determine type of object {}", obj.id());
                debug_assert_eq!("{}", msg);
                msg
            })?;

            match obj_type {
                ObjectType::Commit => {
                    let commit = obj
                        .as_commit()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a commit", obj))
                        .unwrap();
                    debug!("Pushing commit {:?}", commit);

                    let (git_object_hash, minted_ipf_id) =
                        GitObject::from_git_commit(commit, &repo.odb()?)?
                            .chain_add(ipfs, chain_api, signer)
                            .await?;

                    eprintln!(
                        "Minted Git object {} on-chain with IPF ID: {}",
                        git_object_hash, minted_ipf_id
                    );

                    ipf_id_list.push(minted_ipf_id);

                    self.objects.push(format!("{}", obj.id()));
                    debug!(
                        "[{}/{}] Commit {} uploaded to {}",
                        i + 1,
                        oid_count,
                        obj.id(),
                        git_object_hash
                    );
                }
                ObjectType::Tree => {
                    let tree = obj
                        .as_tree()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a tree", obj))
                        .unwrap();
                    debug!("Pushing tree {:?}", tree);

                    let (git_object_hash, minted_ipf_id) =
                        GitObject::from_git_tree(tree, &repo.odb()?)?
                            .chain_add(ipfs, chain_api, signer)
                            .await?;

                    eprintln!(
                        "Minted Git object {} on-chain with IPF ID: {}",
                        git_object_hash, minted_ipf_id
                    );

                    ipf_id_list.push(minted_ipf_id);

                    self.objects.push(format!("{}", obj.id()));
                    debug!(
                        "[{}/{}] Tree {} uploaded to {}",
                        i + 1,
                        oid_count,
                        obj.id(),
                        git_object_hash
                    );
                }
                ObjectType::Blob => {
                    let blob = obj
                        .as_blob()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a blob", obj))
                        .unwrap();
                    debug!("Pushing blob {:?}", blob);

                    let (git_object_hash, minted_ipf_id) =
                        GitObject::from_git_blob(blob, &repo.odb()?)?
                            .chain_add(ipfs, chain_api, signer)
                            .await?;

                    eprintln!(
                        "Minted Git object {} on-chain with IPF ID: {}",
                        git_object_hash, minted_ipf_id
                    );

                    ipf_id_list.push(minted_ipf_id);

                    self.objects.push(format!("{}", obj.id()));
                    debug!(
                        "[{}/{}] Blob {} uploaded to {}",
                        i + 1,
                        oid_count,
                        obj.id(),
                        git_object_hash
                    );
                }
                ObjectType::Tag => {
                    let tag = obj
                        .as_tag()
                        .ok_or_else(|| eprintln!("Could not view {:?} as a tag", obj))
                        .unwrap();
                    debug!("Pushing tag {:?}", tag);

                    let (git_object_hash, minted_ipf_id) =
                        GitObject::from_git_tag(tag, &repo.odb()?)?
                            .chain_add(ipfs, chain_api, signer)
                            .await?;

                    eprintln!(
                        "Minted Git object {} on-chain with IPF ID: {}",
                        git_object_hash, minted_ipf_id
                    );

                    ipf_id_list.push(minted_ipf_id);

                    self.objects.push(format!("{}", obj.id()));

                    debug!(
                        "[{}/{}] Tag {} uploaded to {}",
                        i + 1,
                        oid_count,
                        obj.id(),
                        git_object_hash
                    );
                }
                other => {
                    return Err(format!("Don't know how to traverse a {}", other).into());
                }
            }
        }
        Ok(ipf_id_list)
    }

    /// Download git objects in `oids` from IPFS and instantiate them in `repo`.
    pub async fn fetch_git_objects(
        &self,
        oids: &HashSet<Oid>,
        repo: &mut Repository,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        ips_id: u32,
    ) -> Result<(), Box<dyn Error>> {
        for (i, &oid) in oids.iter().enumerate() {
            debug!("[{}/{}] Fetching object {}", i + 1, oids.len(), oid);

            let obj_git_hash = self
                .objects
                .iter()
                .find(|s| *s == &format!("{}", oid))
                .unwrap_or_else(|| panic!("Could not find object {} in RemoteData", oid));

            let git_obj =
                GitObject::chain_get(obj_git_hash.to_string(), ipfs, chain_api, ips_id).await?;

            if repo.odb()?.read_header(oid).is_ok() {
                debug!("fetch objects: Object {} already present locally!", oid);
                continue;
            }

            let written_oid = repo.odb()?.write(
                match git_obj.metadata {
                    GitObjectMetadata::Blob => ObjectType::Blob,
                    GitObjectMetadata::Commit { .. } => ObjectType::Commit,
                    GitObjectMetadata::Tag { .. } => ObjectType::Tag,
                    GitObjectMetadata::Tree { .. } => ObjectType::Tree,
                },
                &git_obj.raw_data_ipfs_hash,
            )?;
            if written_oid != oid {
                let msg = format!("Object tree inconsistency detected: fetched {} from {}, but write result hashes to {}", oid, obj_git_hash, written_oid);
                debug!("{}", msg);
                return Err(msg.into());
            }
            debug!("Fetched object {} to {}", obj_git_hash, written_oid);
        }
        Ok(())
    }

    pub async fn mint_return_new_old_id(
        &self,
        ipfs: &mut IpfsClient,
        chain_api: &invarch::RuntimeApi<DefaultConfig, PolkadotExtrinsicParams<DefaultConfig>>,
        signer: &PairSigner<DefaultConfig, sp_keyring::sr25519::sr25519::Pair>,
        ips_id: u32,
    ) -> Result<(u64, Option<u64>), Box<dyn Error>> {
        let events = chain_api
            .tx()
            .ipf()
            .mint(
                b"RepoData".to_vec(),
                H256::from_slice(
                    &Cid::try_from(ipfs.add(Cursor::new(self.encode())).await?.hash)?.to_bytes()
                        [2..],
                ),
            )?
            .sign_and_submit_then_watch_default(signer)
            .await?
            .wait_for_finalized_success()
            .await?;

        let new_ipf_id = events
            .find_first::<invarch::ipf::events::Minted>()?
            .unwrap()
            .1;

        eprintln!("Minted Repo Data on-chain with IPF ID: {}", new_ipf_id);

        let ips_info = chain_api
            .storage()
            .inv4()
            .ip_storage(&ips_id, None)
            .await?
            .ok_or(format!("IPS {ips_id} does not exist"))?;

        for file in ips_info.data.0 {
            if let AnyId::IpfId(id) = file {
                let ipf_info = chain_api
                    .storage()
                    .ipf()
                    .ipf_storage(&id, None)
                    .await?
                    .ok_or("Internal error: IPF listed from IPS does not exist")?;
                if String::from_utf8(ipf_info.metadata.0.clone())? == *"RepoData" {
                    return Ok((new_ipf_id, Some(id)));
                }
            }
        }

        Ok((new_ipf_id, None))
    }
}
