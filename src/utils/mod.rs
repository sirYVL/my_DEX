//////////////////////////////////////////
// my_DEX/src/utils/mod.rs
//////////////////////////////////////////

// Dieses mod.rs bindet die Untermodule "hlc" und "geoip_and_ntp" ein,
// sodass extern "use crate::utils::hlc;" m�glich ist.
//
// (c) Ihr DEX-Projekt

pub mod hlc;
pub mod aesgcm_utils;
