//! Shared secret masking, applied at L0 write time (SignalWriter::append) for every source.

/// 粗略遮罩常見 credential/token,避免 secret 隨 dev signal 進腦(codeforge 無共用遮罩
/// 函式;無 regex crate,用 prefix + 高熵長度啟發式,寧可多遮)。
///
/// 兩道:(1) 先整段遮 PEM / OpenSSH 私鑰 block(多行,逐 token 啟發式抓不到 header /
/// 行長 metadata / key 路徑等周邊);(2) 再逐 token 遮已知前綴 / 高熵 token。
pub fn mask_secrets(s: &str) -> String {
    let pem_masked = mask_pem_blocks(s);
    let pem_masked = mask_kv(&mask_bearer(&mask_url_userinfo(&pem_masked)));
    pem_masked
        .split(' ')
        .map(mask_word)
        .collect::<Vec<_>>()
        .join(" ")
}

/// 整段遮蔽 PEM / OpenSSH 私鑰 block:任何含 `BEGIN`+`PRIVATE KEY` 的行起,到含
/// `END`+`PRIVATE KEY` 的行止(含),整段(header / body / footer)收成單一 marker。
///
/// 為何需要:私鑰 base64 body 雖是高熵、token 啟發式或可抓,但 header
/// (`-----BEGIN OPENSSH PRIVATE KEY-----`)、`base64 body 行長` 之類結構 metadata、
/// key 路徑等周邊全漏網;且被 `truncate(300)` 截斷的 block 根本沒有 END 行。逐行整段
/// 遮才穩。無 END(截斷)時,從 BEGIN 行起遮到字串尾(保守,寧可多遮)。
/// codeforge 無 regex crate → 逐行字串比對。
fn mask_pem_blocks(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_key = false;
    for line in s.split('\n') {
        let has_pk = line.contains("PRIVATE KEY");
        if in_key {
            // body / footer 全丟,看到 END 行才退出 redact。
            if has_pk && line.contains("END") {
                in_key = false;
            }
            continue;
        }
        if has_pk && line.contains("BEGIN") {
            out.push("[PRIVATE KEY REDACTED]");
            // 純 BEGIN 行 → 進多行 redact;同行 BEGIN+END(單行表示)罕見,只留 marker。
            if !line.contains("END") {
                in_key = true;
            }
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

/// 遮一個以空白切出的詞:取 `=`/`:` 之後的值部分當候選(處理 KEY=secret /
/// Authorization:token / 裸 token),候選像 secret 就替成 ***。
fn mask_word(word: &str) -> String {
    let val_start = word.rfind(['=', ':']).map(|i| i + 1).unwrap_or(0);
    let (prefix, val_raw) = word.split_at(val_start);
    let candidate =
        val_raw.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-');
    if !candidate.is_empty() && (looks_secret(candidate) || looks_jwt(candidate)) {
        format!("{}{}", prefix, val_raw.replace(candidate, "***"))
    } else {
        word.to_string()
    }
}

/// token 是否像 credential:已知前綴(sk-/ghp_/AKIA/AIza/xox…)或高熵 32+ 字串。
fn looks_secret(t: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "sk-",
        "ghp_",
        "gho_",
        "ghs_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "AKIA",
        "ASIA",
        "AIza",
    ];
    if t.len() >= 12 && PREFIXES.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    // 高熵:32+ 連續 alnum/_/-(不含 '/' 以免遮路徑),且字母+數字混合。
    t.len() >= 32
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

/// JWT-like `a.b.c` (base64url parts), total length >= 20. To spare dotted identifiers
/// (`com.example.app`, versions) it must start with `eyJ` (JWT header) or mix upper case and digits.
fn looks_jwt(t: &str) -> bool {
    let parts: Vec<&str> = t.split('.').collect();
    t.len() >= 20
        && parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        && (t.starts_with("eyJ")
            || (t.chars().any(|c| c.is_ascii_uppercase()) && t.chars().any(|c| c.is_ascii_digit())))
}

/// `scheme://user:pass@host` -> `scheme://***@host` (userinfo without `:` is left alone).
fn mask_url_userinfo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) if tail[..at].contains(':') => {
                out.push_str("***");
                rest = &tail[at..];
            }
            _ => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

/// `Bearer <token>` (any case) -> `Bearer ***`.
fn mask_bearer(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("bearer ") {
        let tok_start = pos + i + 7;
        out.push_str(&s[pos..tok_start]);
        let tok_len = s[tok_start..]
            .find(|c: char| c.is_whitespace() || c == '\'' || c == '"')
            .unwrap_or(s.len() - tok_start);
        if tok_len > 0 && &s[tok_start..tok_start + tok_len] != "***" {
            out.push_str("***");
        } else {
            out.push_str(&s[tok_start..tok_start + tok_len]);
        }
        pos = tok_start + tok_len;
    }
    out.push_str(&s[pos..]);
    out
}

fn secret_key(k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    k.contains("pass")
        || k.contains("secret")
        || k.contains("token")
        || k.contains("apikey")
        || k.contains("api_key")
        || k.contains("api-key")
}

/// `key=value` / `key: value` / `"key": "value"` where key looks secret -> value becomes `***`.
fn mask_kv(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let ident = |x: char| x.is_ascii_alphanumeric() || x == '_' || x == '-';
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if !ident(c[i]) {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let mut k = i;
        while k < c.len() && ident(c[k]) {
            k += 1;
        }
        let key: String = c[i..k].iter().collect();
        out.push_str(&key);
        i = k;
        if !secret_key(&key) {
            continue;
        }
        let mut m = k;
        if m < c.len() && (c[m] == '"' || c[m] == '\'') {
            m += 1;
        }
        while m < c.len() && (c[m] == ' ' || c[m] == '\t') {
            m += 1;
        }
        if m >= c.len() || (c[m] != '=' && c[m] != ':') {
            continue;
        }
        m += 1;
        while m < c.len() && (c[m] == ' ' || c[m] == '\t') {
            m += 1;
        }
        let mut v = m;
        if v < c.len() && (c[v] == '"' || c[v] == '\'') {
            v += 1;
        }
        let mut e = v;
        while e < c.len()
            && !c[e].is_whitespace()
            && !matches!(c[e], '"' | '\'' | ',' | ';' | ')' | '}')
        {
            e += 1;
        }
        let val: String = c[v..e].iter().collect();
        if val.is_empty() || val == "***" {
            continue;
        }
        out.extend(c[k..v].iter());
        out.push_str("***");
        i = e;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_token_prefixes_and_high_entropy() {
        // KEY=token(無空格)前綴遮
        assert_eq!(
            mask_secrets("export GH=ghp_abcdefABCDEF1234567890"),
            "export GH=***"
        );
        // 裸高熵 token 遮
        assert!(mask_secrets("got abcd1234efgh5678ijkl9012mnop3456").contains("***"));
        // 一般文字不遮
        assert_eq!(
            mask_secrets("修了 crontab 的 sed 問題"),
            "修了 crontab 的 sed 問題"
        );
        // 路徑不遮(含 /)
        assert_eq!(
            mask_secrets("檔案 /home/user/projects/mnemos/src"),
            "檔案 /home/user/projects/mnemos/src"
        );
    }

    #[test]
    fn masks_pem_private_key_blocks() {
        // 完整 block:header / body / footer 全遮,前後文保留。
        let full = "ctx before\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXkAAAAB\nMOREKEYBODY9999\n-----END OPENSSH PRIVATE KEY-----\nctx after";
        let m = mask_secrets(full);
        assert!(m.contains("[PRIVATE KEY REDACTED]"), "需有 marker: {m}");
        assert!(!m.contains("BEGIN OPENSSH"), "header 須遮: {m}");
        assert!(!m.contains("b3BlbnNzaC1rZXk"), "body 須遮: {m}");
        assert!(!m.contains("MOREKEYBODY9999"), "body 須遮: {m}");
        assert!(
            m.contains("ctx before") && m.contains("ctx after"),
            "前後文須保留: {m}"
        );

        // 截斷(被 truncate(300) 砍掉 END):從 BEGIN 遮到字串尾。
        let truncated =
            "行數: 8\n-----BEGIN OPENSSH PRIVATE KEY-----   \nbase64 body 行長:\n2: 70\nLEAKTAIL";
        let mt = mask_secrets(truncated);
        assert!(mt.contains("[PRIVATE KEY REDACTED]"));
        assert!(!mt.contains("BEGIN OPENSSH"));
        assert!(!mt.contains("LEAKTAIL"), "截斷後 BEGIN 之後須全遮: {mt}");
        assert!(mt.contains("行數: 8"), "BEGIN 前文字須保留: {mt}");

        // 無 PEM 的一般多行文字不受影響。
        assert_eq!(mask_secrets("line one\nline two"), "line one\nline two");
    }

    #[test]
    fn masks_jwt_like_tokens() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r";
        let m = mask_secrets(&format!("got {jwt} here"));
        assert!(!m.contains("eyJhbGci"), "{m}");
        assert!(m.starts_with("got ") && m.ends_with(" here"), "{m}");
        // dotted identifiers / versions stay
        assert_eq!(
            mask_secrets("see com.example.app and v1.2.3"),
            "see com.example.app and v1.2.3"
        );
    }

    #[test]
    fn masks_bearer_tokens() {
        let m = mask_secrets("curl -H 'Authorization: Bearer abc123xyz' https://x.io");
        assert!(!m.contains("abc123xyz"), "{m}");
        assert!(m.contains("Bearer ***"), "{m}");
        assert!(!mask_secrets("bearer shortsecret").contains("shortsecret"));
    }

    #[test]
    fn masks_url_userinfo() {
        assert_eq!(
            mask_secrets("clone postgres://admin:hunter2@db.local:5432/x now"),
            "clone postgres://***@db.local:5432/x now"
        );
        // no password part -> untouched
        assert_eq!(
            mask_secrets("ssh://git@github.com/a/b"),
            "ssh://git@github.com/a/b"
        );
    }

    #[test]
    fn masks_secret_key_values() {
        for (inp, leak) in [
            ("password=hunter2", "hunter2"),
            ("DB_PASSWORD: s3cr3t", "s3cr3t"),
            ("my_token = abcd", "abcd"),
            ("api_key=zzz999", "zzz999"),
            ("API-KEY: qqq", "qqq"),
            ("{\"client_secret\": \"topsecretvalue\"}", "topsecretvalue"),
            ("apikey=k1k1", "k1k1"),
        ] {
            let m = mask_secrets(inp);
            assert!(!m.contains(leak), "{inp} -> {m}");
            assert!(m.contains("***"), "{inp} -> {m}");
        }
    }

    #[test]
    fn plain_text_and_cjk_unchanged() {
        for s in [
            "fixed the flaky test by retrying once",
            "修了 password 欄位的 CJK 顯示問題,token 數量正常",
            "用量:今天 5 小時視窗 87%\n第二行",
            "key: value in a yaml sample",
        ] {
            assert_eq!(mask_secrets(s), s);
        }
    }

    #[test]
    fn masking_is_idempotent() {
        let once = mask_secrets("password=hunter2 postgres://a:b@h/x Bearer tok123");
        assert_eq!(mask_secrets(&once), once);
    }
}
