use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use chumsky::container::{Container};

use crate::parsing::ast::{Identifier};
use crate::parsing::span::{contains, Span};
use crate::r#static::info::fn_info::FnInfo;
use crate::r#static::info::id_info::IdInfo;
use crate::r#static::info::xs_error::XsError;

#[derive(Debug, Clone)]
pub struct TypeEnv {
    pub groups: HashSet<String>,
    pub identifiers: HashMap<Identifier, IdInfo>,
    pub fn_envs: HashMap<Identifier, Vec<FnInfo>>,
    
    pub errs: HashMap<PathBuf, Vec<XsError>>,

    pub current_doc: Option<String>,
    pub current_fnv_env: Option<FnInfo>, // mmm...
    
    pub current_ignores: Arc<RwLock<Option<HashSet<u32>>>>,
    
    pub include_dirs: Arc<Vec<PathBuf>>,
    pub dependencies: Option<HashMap<PathBuf, HashSet<PathBuf>>>,
}

pub struct TempIgnore {
    ignores: Arc<RwLock<Option<HashSet<u32>>>>,
}

impl Drop for TempIgnore {
    fn drop(&mut self) {
        self.ignores.write().expect("Not concurrent").take();
    }
}

impl TypeEnv {
    
    pub fn errs(&self) -> &HashMap<PathBuf, Vec<XsError>> {
        &self.errs
    }
    
    pub fn new(include_dirs: Vec<PathBuf>) -> Self {
        Self {
            groups: HashSet::new(),
            identifiers: HashMap::new(),
            fn_envs: HashMap::new(),
            errs: HashMap::new(),

            include_dirs: Arc::new(include_dirs),
            dependencies: Some(HashMap::new()),

            current_ignores: Arc::new(RwLock::new(None)),
            
            current_doc: None,
            current_fnv_env: None,
        }
    }

    pub fn is_warning_ignored(&self, ignore: u32) -> bool {
        let current_ignores = self.current_ignores.read().expect("Not concurrent");
        let Some(ignores) = current_ignores.as_ref() else {
            return false;
        };
        ignores.contains(&ignore)
    }
    
    pub fn temp_ignore(&mut self, ignores: HashSet<u32>) -> TempIgnore {
        self.current_ignores.write().expect("Not concurrent").replace(ignores);
        TempIgnore { ignores: self.current_ignores.clone() }
    }
    
    pub fn get_mut(&mut self, id: &Identifier) -> Option<&mut IdInfo> {
        self.current_fnv_env.as_mut()
            .and_then(|env| env.get_mut(id))
            .or_else(|| self.identifiers.get_mut(id))
    }

    pub fn get_ref(&self, id: &Identifier) -> Option<&IdInfo> {
        self.current_fnv_env.as_ref()
            .and_then(|env| env.get(id))
            .or_else(|| self.identifiers.get(id))
    }

    pub fn get(&self, id: &Identifier) -> Option<IdInfo> {
        self.current_fnv_env.as_ref()
            .and_then(|env| env.get(id))
            .or_else(|| self.identifiers.get(id)).cloned()
    }

    pub fn pop(&mut self, id: &Identifier) -> Option<IdInfo> {
        self.current_fnv_env.as_mut()
            .and_then(|env| env.pop(id))
            .or_else(|| self.identifiers.remove(id))
    }
    
    pub fn set(&mut self, id: &Identifier, info: IdInfo) {
        match &mut self.current_fnv_env {
            Some(env) => env.set(id.clone(), info),
            None => self.identifiers.push((id.clone(), info)),
        }
    }

    pub fn set_global(&mut self, id: &Identifier, info: IdInfo) {
        self.identifiers.push((id.clone(), info))
    }
    
    pub fn get_return(&self) -> Option<IdInfo> {
        self.current_fnv_env.as_ref()
            .and_then(|env| env.get(&Identifier::new("return"))).cloned()
    }
    
    pub fn add_group(&mut self, group: &String) {
        self.groups.insert(group.clone());
    }

    fn process_err(&mut self, err: &mut XsError) {
        if !err.is_warning() {
            return;
        }
        let to_ignore = self.is_warning_ignored(err.code());
        match err {
            XsError::Warning { ignored, .. } => *ignored = to_ignore,
            _ => unreachable!("is_warning check above"),
        }
    }
    
    pub fn add_err(&mut self, path: &PathBuf, mut err: XsError) {
        self.process_err(&mut err);
        
        self.errs
            .entry(path.clone())
            .or_insert(vec![])
            .push(err);
    }
    
    pub fn add_errs(&mut self, path: &PathBuf, mut errs: Vec<XsError>) {
        for err in errs.iter_mut() {
            self.process_err(err);
        }
        
        self.errs
            .entry(path.clone())
            .or_insert(vec![])
            .extend(errs);
    }
    
    pub fn set_fn_env(&mut self, fn_info: FnInfo) {
        self.current_fnv_env = Some(fn_info)
    }
    
    pub fn get_fn_env(&mut self) -> Option<FnInfo> {
        self.current_fnv_env.take()
    }
    
    pub fn save_fn_env(&mut self, name: &Identifier) {
        let fn_env = self.current_fnv_env.take().expect("No current fn env - Bugged call");
        self.fn_envs
            .entry(name.clone())
            .or_insert(vec![])
            .push(fn_env);
    }

    pub fn set_doc(&mut self, doc: String) {
        self.current_doc = Some(doc);
    }

    pub fn take_doc(&mut self) -> Option<String> {
        self.current_doc.take()
    }
    
    pub fn local_ids(&self, path: &PathBuf, span: &Span) -> Option<&HashMap<Identifier, IdInfo>> {
        self.fn_envs
            .values()
            .flatten()
            .filter(|env| {
                let loc = &env.src_loc;
                loc.file_path == *path && contains(&loc.span, span)
            }).map(|env| &env.identifiers).next()
    }
}