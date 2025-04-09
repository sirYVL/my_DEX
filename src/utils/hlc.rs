//////////////////////////////////////////////////////
// my_DEX/src/utils/hlc.rs
//////////////////////////////////////////////////////

// Produktionsreifer Code für Hybrid Logical Clocks und
// umfassende NTP-Pool-Listen (global, Kontinente, einzelne Länder).
// Alle Arrays jetzt als pub static, damit extern importierbar.
//
// (c) Ihr DEX-Projekt

use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::OnceCell;
use tokio::task;
use tokio::time::{timeout, Duration};
use anyhow::{Result, anyhow};
use sntpc::{self, NtpTimestampGenerator};
use tracing::{info, debug, warn};

// ----------------------------------------------------------------------
// GLOBAL_TIME_CACHE => z. B. aggregated NTP time in ms
pub static GLOBAL_TIME_CACHE: OnceCell<u64> = OnceCell::new();

#[derive(Debug, Clone)]
pub struct HlcState {
    pub logical_clock: u64,
    pub last_physical_ms: u64,
}

impl Default for HlcState {
    fn default() -> Self {
        HlcState {
            logical_clock: 0,
            last_physical_ms: 0,
        }
    }
}

// ----------------------------------------------------------------------
// Weltweite NTP-Pools nach Kontinent, plus globale Pools.
// Alle "pub static" => extern zugreifbar
// ----------------------------------------------------------------------
pub static GLOBAL_POOLS: &[&str] = &[
    "pool.ntp.org",
    "0.pool.ntp.org",
    "1.pool.ntp.org",
    "2.pool.ntp.org",
    "3.pool.ntp.org",
];

pub static AFRICA_POOLS: &[&str] = &[
    "africa.pool.ntp.org",
    "0.africa.pool.ntp.org",
    "1.africa.pool.ntp.org",
    "2.africa.pool.ntp.org",
    "3.africa.pool.ntp.org",
    "ao.pool.ntp.org","bf.pool.ntp.org","bi.pool.ntp.org","bj.pool.ntp.org","bw.pool.ntp.org",
    "cd.pool.ntp.org","cf.pool.ntp.org","cg.pool.ntp.org","ci.pool.ntp.org","cm.pool.ntp.org",
    "cv.pool.ntp.org","dj.pool.ntp.org","dz.pool.ntp.org","eg.pool.ntp.org","eh.pool.ntp.org",
    "er.pool.ntp.org","et.pool.ntp.org","ga.pool.ntp.org","gh.pool.ntp.org","gm.pool.ntp.org",
    "gn.pool.ntp.org","gq.pool.ntp.org","gw.pool.ntp.org","ke.pool.ntp.org","km.pool.ntp.org",
    "lr.pool.ntp.org","ls.pool.ntp.org","ly.pool.ntp.org","ma.pool.ntp.org","mg.pool.ntp.org",
    "ml.pool.ntp.org","mr.pool.ntp.org","mu.pool.ntp.org","mw.pool.ntp.org","mz.pool.ntp.org",
    "na.pool.ntp.org","ne.pool.ntp.org","ng.pool.ntp.org","re.pool.ntp.org","rw.pool.ntp.org",
    "sc.pool.ntp.org","sd.pool.ntp.org","sh.pool.ntp.org","sl.pool.ntp.org","sn.pool.ntp.org",
    "so.pool.ntp.org","ss.pool.ntp.org","st.pool.ntp.org","sz.pool.ntp.org","td.pool.ntp.org",
    "tg.pool.ntp.org","tn.pool.ntp.org","tz.pool.ntp.org","ug.pool.ntp.org","yt.pool.ntp.org",
    "za.pool.ntp.org","zm.pool.ntp.org","zw.pool.ntp.org",
];

pub static ANTARCTICA_POOLS: &[&str] = &[
    "antarctica.pool.ntp.org",
    // laut Liste => 0.pool.ntp.org etc. identisch global?
    "0.pool.ntp.org",
    "1.pool.ntp.org",
    "2.pool.ntp.org",
    "3.pool.ntp.org",
    "aq.pool.ntp.org","bv.pool.ntp.org","gs.pool.ntp.org","hm.pool.ntp.org","tf.pool.ntp.org",
];

pub static ASIA_POOLS: &[&str] = &[
    "asia.pool.ntp.org",
    "0.asia.pool.ntp.org",
    "1.asia.pool.ntp.org",
    "2.asia.pool.ntp.org",
    "3.asia.pool.ntp.org",
    "ae.pool.ntp.org","af.pool.ntp.org","am.pool.ntp.org","az.pool.ntp.org","bd.pool.ntp.org",
    "bh.pool.ntp.org","bn.pool.ntp.org","bt.pool.ntp.org","cc.pool.ntp.org","cn.pool.ntp.org",
    "ge.pool.ntp.org","hk.pool.ntp.org","id.pool.ntp.org","il.pool.ntp.org","in.pool.ntp.org",
    "io.pool.ntp.org","iq.pool.ntp.org","ir.pool.ntp.org","jo.pool.ntp.org","jp.pool.ntp.org",
    "kg.pool.ntp.org","kh.pool.ntp.org","kp.pool.ntp.org","kr.pool.ntp.org","kw.pool.ntp.org",
    "kz.pool.ntp.org","la.pool.ntp.org","lb.pool.ntp.org","lk.pool.ntp.org","mm.pool.ntp.org",
    "mn.pool.ntp.org","mo.pool.ntp.org","mv.pool.ntp.org","my.pool.ntp.org","np.pool.ntp.org",
    "om.pool.ntp.org","ph.pool.ntp.org","pk.pool.ntp.org","ps.pool.ntp.org","qa.pool.ntp.org",
    "sa.pool.ntp.org","sg.pool.ntp.org","sy.pool.ntp.org","th.pool.ntp.org","tj.pool.ntp.org",
    "tm.pool.ntp.org","tw.pool.ntp.org","uz.pool.ntp.org","vn.pool.ntp.org","ye.pool.ntp.org",
];

pub static EUROPE_POOLS: &[&str] = &[
    "europe.pool.ntp.org",
    "0.europe.pool.ntp.org",
    "1.europe.pool.ntp.org",
    "2.europe.pool.ntp.org",
    "3.europe.pool.ntp.org",
    "ad.pool.ntp.org","al.pool.ntp.org","at.pool.ntp.org","ax.pool.ntp.org","ba.pool.ntp.org",
    "be.pool.ntp.org","bg.pool.ntp.org","by.pool.ntp.org","ch.pool.ntp.org","cy.pool.ntp.org",
    "cz.pool.ntp.org","de.pool.ntp.org","dk.pool.ntp.org","ee.pool.ntp.org","es.pool.ntp.org",
    "fi.pool.ntp.org","fo.pool.ntp.org","fr.pool.ntp.org","gg.pool.ntp.org","gi.pool.ntp.org",
    "gr.pool.ntp.org","hr.pool.ntp.org","hu.pool.ntp.org","ie.pool.ntp.org","im.pool.ntp.org",
    "is.pool.ntp.org","it.pool.ntp.org","je.pool.ntp.org","li.pool.ntp.org","lt.pool.ntp.org",
    "lu.pool.ntp.org","lv.pool.ntp.org","mc.pool.ntp.org","md.pool.ntp.org","me.pool.ntp.org",
    "mk.pool.ntp.org","mt.pool.ntp.org","nl.pool.ntp.org","no.pool.ntp.org","pl.pool.ntp.org",
    "pt.pool.ntp.org","ro.pool.ntp.org","rs.pool.ntp.org","ru.pool.ntp.org","se.pool.ntp.org",
    "si.pool.ntp.org","sj.pool.ntp.org","sk.pool.ntp.org","sm.pool.ntp.org","tr.pool.ntp.org",
    "ua.pool.ntp.org","uk.pool.ntp.org","va.pool.ntp.org","xk.pool.ntp.org","yu.pool.ntp.org",
];

pub static NORTH_AMERICA_POOLS: &[&str] = &[
    "north-america.pool.ntp.org",
    "0.north-america.pool.ntp.org",
    "1.north-america.pool.ntp.org",
    "2.north-america.pool.ntp.org",
    "3.north-america.pool.ntp.org",
    "ag.pool.ntp.org","ai.pool.ntp.org","aw.pool.ntp.org","bb.pool.ntp.org","bl.pool.ntp.org",
    "bm.pool.ntp.org","bq.pool.ntp.org","bs.pool.ntp.org","bz.pool.ntp.org","ca.pool.ntp.org",
    "cr.pool.ntp.org","cu.pool.ntp.org","cw.pool.ntp.org","dm.pool.ntp.org","do.pool.ntp.org",
    "gd.pool.ntp.org","gl.pool.ntp.org","gp.pool.ntp.org","gt.pool.ntp.org","hn.pool.ntp.org",
    "ht.pool.ntp.org","jm.pool.ntp.org","kn.pool.ntp.org","ky.pool.ntp.org","lc.pool.ntp.org",
    "mf.pool.ntp.org","mq.pool.ntp.org","ms.pool.ntp.org","mx.pool.ntp.org","ni.pool.ntp.org",
    "pa.pool.ntp.org","pm.pool.ntp.org","pr.pool.ntp.org","sv.pool.ntp.org","sx.pool.ntp.org",
    "tc.pool.ntp.org","tt.pool.ntp.org","us.pool.ntp.org","vc.pool.ntp.org","vg.pool.ntp.org",
    "vi.pool.ntp.org",
];

pub static OCEANIA_POOLS: &[&str] = &[
    "oceania.pool.ntp.org",
    "0.oceania.pool.ntp.org",
    "1.oceania.pool.ntp.org",
    "2.oceania.pool.ntp.org",
    "3.oceania.pool.ntp.org",
    "as.pool.ntp.org","au.pool.ntp.org","ck.pool.ntp.org","cx.pool.ntp.org","fj.pool.ntp.org",
    "fm.pool.ntp.org","gu.pool.ntp.org","ki.pool.ntp.org","mh.pool.ntp.org","mp.pool.ntp.org",
    "nc.pool.ntp.org","nf.pool.ntp.org","nr.pool.ntp.org","nu.pool.ntp.org","nz.pool.ntp.org",
    "pf.pool.ntp.org","pg.pool.ntp.org","pn.pool.ntp.org","pw.pool.ntp.org","sb.pool.ntp.org",
    "tk.pool.ntp.org","tl.pool.ntp.org","to.pool.ntp.org","tv.pool.ntp.org","um.pool.ntp.org",
    "vu.pool.ntp.org","wf.pool.ntp.org","ws.pool.ntp.org",
];

pub static SOUTH_AMERICA_POOLS: &[&str] = &[
    "south-america.pool.ntp.org",
    "0.south-america.pool.ntp.org",
    "1.south-america.pool.ntp.org",
    "2.south-america.pool.ntp.org",
    "3.south-america.pool.ntp.org",
    "ar.pool.ntp.org","bo.pool.ntp.org","br.pool.ntp.org","cl.pool.ntp.org","co.pool.ntp.org",
    "ec.pool.ntp.org","fk.pool.ntp.org","gf.pool.ntp.org","gy.pool.ntp.org","pe.pool.ntp.org",
    "py.pool.ntp.org","sr.pool.ntp.org","uy.pool.ntp.org","ve.pool.ntp.org",
];

// ----------------------------------------------------------------------
// Optionale HLC-Funktionen: z. B. update_hlc, replicate_state...
// (ggf. weglassen, falls Sie in geoip_and_ntp erst definieren wollen)
// ----------------------------------------------------------------------

/// Aggregiert Zeitstempel => z. B. Median (kann man extern nutzen)
pub fn aggregate_time(timestamps: &[u64]) -> u64 {
    if timestamps.is_empty() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap_or_default()
            .as_millis() as u64;
        return now;
    }
    let mut tmp = timestamps.to_vec();
    tmp.sort_unstable();
    let mid = tmp.len() / 2;
    if tmp.len() % 2 == 1 {
        tmp[mid]
    } else {
        let v1 = tmp[mid - 1];
        let v2 = tmp[mid];
        (v1 + v2) / 2
    }
}

/// HLC-Update
pub fn update_hlc(logical_clock: &mut u64, last_physical_ms: &mut u64, remote_time: u64) {
    let local_ms = if let Some(ntp_ms) = GLOBAL_TIME_CACHE.get() {
        *ntp_ms
    } else {
        let st = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap_or_default()
            .as_millis() as u64;
        st
    };

    let physical_now = std::cmp::max(local_ms, remote_time);

    if physical_now < *last_physical_ms {
        *logical_clock += 1;
    } else {
        *logical_clock = 0;
    }
    let new_ts = std::cmp::max(physical_now, *last_physical_ms);
    *last_physical_ms = new_ts;
}

/// Schreibt new_time in die GLOBAL_TIME_CACHE
pub fn update_cache(new_time_ms: u64) {
    // ignore error if already set
    GLOBAL_TIME_CACHE.set(new_time_ms).ok();
    debug!("GLOBAL_TIME_CACHE updated => {}", new_time_ms);
}

// Ende hlc.rs
