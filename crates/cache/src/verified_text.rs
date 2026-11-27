//! Compile string equality through the selected vstd implementation.
#![allow(unexpected_cfgs)]
use vstd::contrib::exec_spec::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

exec_spec_verified! {
    pub open spec fn same_text(left: SpecString, right: SpecString) -> bool {
        left == right
    }
}
