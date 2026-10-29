//! Secure channel for remote devices. Byte-compatible with packages/protocol/src/channel.ts.
//! Both sides prove they hold a pre-shared key (pairing code or device secret) and agree on
//! fresh AES-256-GCM keys through an ephemeral P-256 exchange.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use p256::elliptic_curve::Generate;
use p256::{PublicKey, SecretKey, ecdh::diffie_hellman, elliptic_curve::sec1::ToSec1Point};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL: &str = "phonedeck/1";
pub const MAX_FRAME: usize = 2 * 1024 * 1024;
const MAX_COUNTER: u64 = 1 << 32;
const DIR_C2S: u8 = 1;
const DIR_S2C: u8 = 2;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("malformed handshake message")]
    Malformed,
    #[error("peer could not prove the key")]
    BadProof,
    #[error("bad frame")]
    BadFrame,
    #[error("session exhausted")]
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Pair,
    Resume,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Pair => "pair",
            Mode::Resume => "resume",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    pub kind: String,
    pub app: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub t: String,
    pub v: u8,
    pub mode: Mode,
    pub cid: String,
    pub id: String,
    pub e: String,
    pub n: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub t: String,
    pub e: String,
    pub n: String,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confirm {
    pub t: String,
    pub mac: String,
}

pub fn b64(b: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(b)
}

pub fn unb64(s: &str, len: Option<usize>) -> Result<Vec<u8>, ChannelError> {
    let v = URL_SAFE_NO_PAD.decode(s).map_err(|_| ChannelError::Malformed)?;
    match len {
        Some(n) if v.len() != n => Err(ChannelError::Malformed),
        _ => Ok(v),
    }
}

fn lp(out: &mut Vec<u8>, b: &[u8]) {
    let n = u16::try_from(b.len()).expect("field fits in u16");
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(b);
}

#[allow(clippy::too_many_arguments)]
fn transcript(mode: Mode, cid: &[u8], id: &[u8], ec: &[u8], nc: &[u8], es: &[u8], ns: &[u8]) -> Vec<u8> {
    let mut t = Vec::with_capacity(256);
    for f in [PROTOCOL.as_bytes(), mode.as_str().as_bytes(), cid, id, ec, nc, es, ns] {
        lp(&mut t, f);
    }
    t
}

struct Derived {
    th: [u8; 32],
    c2s: [u8; 32],
    s2c: [u8; 32],
    mac_s: [u8; 32],
    mac_c: [u8; 32],
    sas: String,
}

impl Drop for Derived {
    fn drop(&mut self) {
        for k in [&mut self.c2s, &mut self.s2c, &mut self.mac_s, &mut self.mac_c] {
            k.fill(0);
        }
    }
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut m = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().into()
}

fn verify(key: &[u8], parts: &[&[u8]], tag: &[u8]) -> bool {
    let mut m = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    for p in parts {
        m.update(p);
    }
    m.verify_slice(tag).is_ok()
}

fn derive(psk: &[u8], shared: &[u8], transcript: &[u8]) -> Derived {
    let th: [u8; 32] = Sha256::digest(transcript).into();
    let mut info = Vec::with_capacity(64);
    lp(&mut info, format!("{PROTOCOL} keys").as_bytes());
    info.extend_from_slice(&th);
    let mut okm = [0u8; 128];
    Hkdf::<Sha256>::new(Some(psk), shared).expand(&info, &mut okm).expect("128 bytes is a valid hkdf length");
    let mut d = Derived { th, c2s: [0; 32], s2c: [0; 32], mac_s: [0; 32], mac_c: [0; 32], sas: String::new() };
    d.c2s.copy_from_slice(&okm[0..32]);
    d.s2c.copy_from_slice(&okm[32..64]);
    d.mac_s.copy_from_slice(&okm[64..96]);
    d.mac_c.copy_from_slice(&okm[96..128]);
    okm.fill(0);
    let s = hmac(&d.mac_s, &[b"sas", &d.th]);
    d.sas = format!("{:06}", u32::from_be_bytes([s[0], s[1], s[2], s[3]]) % 1_000_000);
    d
}

fn ecdh(secret: &SecretKey, peer: &[u8]) -> Result<[u8; 32], ChannelError> {
    if peer.len() != 65 || peer[0] != 4 {
        return Err(ChannelError::Malformed);
    }
    let pk = PublicKey::from_sec1_bytes(peer).map_err(|_| ChannelError::Malformed)?;
    let shared = diffie_hellman(secret.to_nonzero_scalar(), pk.as_affine());
    let mut out = [0u8; 32];
    out.copy_from_slice(shared.raw_secret_bytes());
    Ok(out)
}

fn new_secret() -> SecretKey {
    SecretKey::try_generate_from_rng(&mut rand::rngs::SysRng).expect("operating system random number generator")
}

fn public_raw(secret: &SecretKey) -> Vec<u8> {
    secret.public_key().to_sec1_point(false).as_bytes().to_vec()
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    crate::ids::fill_random(&mut b);
    b
}

/// Fixed ephemeral key and nonce, for test vectors only.
pub struct Fixed {
    pub secret: SecretKey,
    pub nonce: [u8; 16],
}

pub struct ServerHandshake {
    d: Derived,
}

impl ServerHandshake {
    /// `cid` and `psk` must belong to the pairing or device named by `hello.id`.
    pub fn respond(hello: &Hello, cid: &[u8], psk: &[u8], fixed: Option<Fixed>) -> Result<(Self, Challenge), ChannelError> {
        if hello.t != "hello" || hello.v != 1 {
            return Err(ChannelError::Malformed);
        }
        let ec = unb64(&hello.e, Some(65))?;
        let nc = unb64(&hello.n, Some(16))?;
        let id = unb64(&hello.id, None)?;
        if id.is_empty() || id.len() > 64 {
            return Err(ChannelError::Malformed);
        }
        let (secret, ns) = match fixed {
            Some(f) => (f.secret, f.nonce),
            None => (new_secret(), random::<16>()),
        };
        let es = public_raw(&secret);
        let shared = ecdh(&secret, &ec)?;
        let d = derive(psk, &shared, &transcript(hello.mode, cid, &id, &ec, &nc, &es, &ns));
        let mac = hmac(&d.mac_s, &[b"server", &d.th]);
        let ch = Challenge { t: "challenge".into(), e: b64(&es), n: b64(&ns), mac: b64(&mac) };
        Ok((Self { d }, ch))
    }

    pub fn sas(&self) -> &str {
        &self.d.sas
    }

    pub fn accept(self, c: &Confirm) -> Result<Session, ChannelError> {
        if c.t != "confirm" {
            return Err(ChannelError::Malformed);
        }
        let tag = unb64(&c.mac, Some(32))?;
        if !verify(&self.d.mac_c, &[b"client", &self.d.th], &tag) {
            return Err(ChannelError::BadProof);
        }
        Ok(Session::new(&self.d.s2c, &self.d.c2s, DIR_S2C, DIR_C2S))
    }
}

/// Client side. Used by tests and by the companion when it talks to another companion.
pub struct ClientHandshake {
    mode: Mode,
    cid: Vec<u8>,
    id: Vec<u8>,
    psk: Vec<u8>,
    secret: SecretKey,
    e: Vec<u8>,
    n: [u8; 16],
}

impl ClientHandshake {
    pub fn start(mode: Mode, cid: &[u8], id: &[u8], psk: &[u8], device: Option<DeviceInfo>, fixed: Option<Fixed>) -> (Self, Hello) {
        let (secret, n) = match fixed {
            Some(f) => (f.secret, f.nonce),
            None => (new_secret(), random::<16>()),
        };
        let e = public_raw(&secret);
        let hello = Hello { t: "hello".into(), v: 1, mode, cid: b64(cid), id: b64(id), e: b64(&e), n: b64(&n), device };
        (Self { mode, cid: cid.to_vec(), id: id.to_vec(), psk: psk.to_vec(), secret, e, n }, hello)
    }

    pub fn finish(self, ch: &Challenge) -> Result<(Session, Confirm, String), ChannelError> {
        if ch.t != "challenge" {
            return Err(ChannelError::Malformed);
        }
        let es = unb64(&ch.e, Some(65))?;
        let ns = unb64(&ch.n, Some(16))?;
        let shared = ecdh(&self.secret, &es)?;
        let d = derive(&self.psk, &shared, &transcript(self.mode, &self.cid, &self.id, &self.e, &self.n, &es, &ns));
        if !verify(&d.mac_s, &[b"server", &d.th], &unb64(&ch.mac, Some(32))?) {
            return Err(ChannelError::BadProof);
        }
        let mac = hmac(&d.mac_c, &[b"client", &d.th]);
        let session = Session::new(&d.c2s, &d.s2c, DIR_C2S, DIR_S2C);
        Ok((session, Confirm { t: "confirm".into(), mac: b64(&mac) }, d.sas.clone()))
    }
}

/// Encrypted, ordered, replay-proof message stream. Owned by one task, so no locking.
pub struct Session {
    send: Aes256Gcm,
    recv: Aes256Gcm,
    send_dir: u8,
    recv_dir: u8,
    send_ctr: u64,
    recv_ctr: u64,
}

impl Session {
    fn new(send: &[u8; 32], recv: &[u8; 32], send_dir: u8, recv_dir: u8) -> Self {
        Self {
            send: Aes256Gcm::new_from_slice(send).expect("32 byte key"),
            recv: Aes256Gcm::new_from_slice(recv).expect("32 byte key"),
            send_dir,
            recv_dir,
            send_ctr: 0,
            recv_ctr: 0,
        }
    }

    fn nonce(ctr: u64) -> [u8; 12] {
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&ctr.to_be_bytes());
        n
    }

    pub fn seal(&mut self, plain: &[u8]) -> Result<Vec<u8>, ChannelError> {
        if self.send_ctr >= MAX_COUNTER {
            return Err(ChannelError::Exhausted);
        }
        let ctr = self.send_ctr;
        let n = Self::nonce(ctr);
        let ct = self.send.encrypt(&Nonce::from(n), Payload { msg: plain, aad: &[self.send_dir] }).map_err(|_| ChannelError::BadFrame)?;
        if ct.len() + 8 > MAX_FRAME {
            return Err(ChannelError::BadFrame);
        }
        self.send_ctr += 1;
        let mut out = Vec::with_capacity(8 + ct.len());
        out.extend_from_slice(&ctr.to_be_bytes());
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Any error means the stream is broken: close the connection.
    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>, ChannelError> {
        if frame.len() < 8 + 16 || frame.len() > MAX_FRAME {
            return Err(ChannelError::BadFrame);
        }
        let ctr = u64::from_be_bytes(frame[..8].try_into().expect("8 bytes"));
        if ctr != self.recv_ctr {
            return Err(ChannelError::BadFrame);
        }
        let n = Self::nonce(ctr);
        let pt = self.recv.decrypt(&Nonce::from(n), Payload { msg: &frame[8..], aad: &[self.recv_dir] }).map_err(|_| ChannelError::BadFrame)?;
        self.recv_ctr += 1;
        Ok(pt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/vectors/channel.json");
        serde_json::from_str(&std::fs::read_to_string(p).expect("vectors file")).expect("json")
    }

    fn jwk_secret(j: &Value) -> SecretKey {
        SecretKey::from_slice(&unb64(j["d"].as_str().unwrap(), Some(32)).unwrap()).unwrap()
    }

    fn arr16(s: &str) -> [u8; 16] {
        unb64(s, Some(16)).unwrap().try_into().unwrap()
    }

    #[test]
    fn matches_typescript_vectors() {
        let v = vectors();
        for c in v["cases"].as_array().unwrap() {
            let mode: Mode = serde_json::from_value(c["mode"].clone()).unwrap();
            let psk = unb64(c["psk"].as_str().unwrap(), None).unwrap();
            let cid = unb64(c["cid"].as_str().unwrap(), None).unwrap();
            let id = unb64(c["id"].as_str().unwrap(), None).unwrap();
            let (client, hello) =
                ClientHandshake::start(mode, &cid, &id, &psk, None, Some(Fixed { secret: jwk_secret(&c["client_jwk"]), nonce: arr16(c["client_nonce"].as_str().unwrap()) }));
            assert_eq!(hello.e, c["hello_e"].as_str().unwrap());
            let (server, ch) =
                ServerHandshake::respond(&hello, &cid, &psk, Some(Fixed { secret: jwk_secret(&c["server_jwk"]), nonce: arr16(c["server_nonce"].as_str().unwrap()) })).unwrap();
            assert_eq!(ch.e, c["challenge_e"].as_str().unwrap());
            assert_eq!(ch.mac, c["challenge_mac"].as_str().unwrap());
            assert_eq!(server.sas(), c["sas"].as_str().unwrap());
            let (mut cs, confirm, sas) = client.finish(&ch).unwrap();
            assert_eq!(sas, c["sas"].as_str().unwrap());
            assert_eq!(confirm.mac, c["confirm_mac"].as_str().unwrap());
            let mut ss = server.accept(&confirm).unwrap();

            let c2s = cs.seal(c["first_c2s"]["plain"].as_str().unwrap().as_bytes()).unwrap();
            assert_eq!(b64(&c2s), c["first_c2s"]["frame"].as_str().unwrap());
            assert_eq!(ss.open(&c2s).unwrap(), c["first_c2s"]["plain"].as_str().unwrap().as_bytes());
            let s2c = ss.seal(c["first_s2c"]["plain"].as_str().unwrap().as_bytes()).unwrap();
            assert_eq!(b64(&s2c), c["first_s2c"]["frame"].as_str().unwrap());
        }
    }

    #[test]
    fn rejects_wrong_psk_replay_and_tampering() {
        let cid = [7u8; 16];
        let (client, hello) = ClientHandshake::start(Mode::Resume, &cid, &[1; 16], &[9; 32], None, None);
        let (_, ch) = ServerHandshake::respond(&hello, &cid, &[8; 32], None).unwrap();
        assert!(matches!(client.finish(&ch), Err(ChannelError::BadProof)));

        let (client, hello) = ClientHandshake::start(Mode::Resume, &cid, &[1; 16], &[9; 32], None, None);
        let (server, ch) = ServerHandshake::respond(&hello, &cid, &[9; 32], None).unwrap();
        let (mut cs, confirm, _) = client.finish(&ch).unwrap();
        let mut ss = server.accept(&confirm).unwrap();
        let f = cs.seal(b"{}").unwrap();
        assert!(ss.open(&f).is_ok());
        assert!(ss.open(&f).is_err(), "replay");
        let mut g = cs.seal(b"{}").unwrap();
        let last = g.len() - 1;
        g[last] ^= 1;
        assert!(ss.open(&g).is_err(), "tamper");
        let h = cs.seal(b"{}").unwrap();
        assert!(cs.open(&h).is_err(), "reflection");
    }

    #[test]
    fn confirm_from_another_session_is_rejected() {
        let cid = [3u8; 16];
        let psk = [5u8; 16];
        let (c1, h1) = ClientHandshake::start(Mode::Pair, &cid, &[1; 8], &psk, None, None);
        let (_s1, ch1) = ServerHandshake::respond(&h1, &cid, &psk, None).unwrap();
        let (_, confirm1, _) = c1.finish(&ch1).unwrap();
        let (_s2, _) = ServerHandshake::respond(&h1, &cid, &psk, None).unwrap();
        assert!(matches!(_s2.accept(&confirm1), Err(ChannelError::BadProof)));
    }
}
