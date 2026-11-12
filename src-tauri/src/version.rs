//! 版本号比较。
//!
//! 只服务更新检查：判断 GitHub Release 的 tag 是否比当前运行版本新。
//! 按语义化版本 2.0 的规则比较，不引入第三方 crate——需要的只是
//! 三段数字加预发布标识，规则固定且必须可单测。

use std::cmp::Ordering;

/// 解析后的版本号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    /// 预发布标识（`2.0.1-beta.1` 的 `beta.1`）。空表示正式版。
    pre: Vec<String>,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| cmp_pre(&self.pre, &other.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 预发布标识的比较规则（semver 2.0 第 11 条）。
///
/// 没有预发布标识的版本更高；标识逐段比较，纯数字按数值比、
/// 数字低于字母，前缀相同时段数多的更高。
fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.iter().zip(b.iter()) {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(nx), Ok(ny)) => nx.cmp(&ny),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            a.len().cmp(&b.len())
        }
    }
}

/// 缺省段按 0 处理；非数字返回 `None`。
fn num(part: Option<&str>) -> Option<u64> {
    match part {
        None => Some(0),
        Some(s) => s.parse().ok(),
    }
}

/// 解析版本号。接受 `v` 前缀、缺省的小版本与修订号、`+build` 后缀。
///
/// 返回 `None` 表示无法解析。调用方应当把它当作「不构成更新」而不是报错：
/// 更新检查失败不该打扰用户。
pub fn parse(raw: &str) -> Option<Version> {
    let text = raw.trim();
    let text = text
        .strip_prefix('v')
        .or_else(|| text.strip_prefix('V'))
        .unwrap_or(text);
    // build 元数据不参与比较。
    let text = text.split('+').next().unwrap_or(text);

    let (core, pre) = match text.split_once('-') {
        Some((c, p)) => (
            c,
            p.split('.')
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        ),
        None => (text, Vec::new()),
    };

    let mut parts = core.split('.');
    let major = num(parts.next())?;
    let minor = num(parts.next())?;
    let patch = num(parts.next())?;
    if parts.next().is_some() {
        return None;
    }

    Some(Version {
        major,
        minor,
        patch,
        pre,
    })
}

/// `latest` 是否比 `current` 新。
///
/// 任一方解析失败都返回 `false`：宁可漏报一次更新，也不要在界面上
/// 提示一个来路不明的版本号。
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_prefixed_versions() {
        assert_eq!(
            parse("2.0.1"),
            Some(Version {
                major: 2,
                minor: 0,
                patch: 1,
                pre: vec![]
            })
        );
        assert_eq!(parse("v2.0.1"), parse("2.0.1"));
        assert_eq!(parse("V2.0.1"), parse("2.0.1"));
        assert_eq!(parse(" 2.0.1 "), parse("2.0.1"));
    }

    #[test]
    fn missing_segments_default_to_zero() {
        assert_eq!(parse("2"), parse("2.0.0"));
        assert_eq!(parse("2.1"), parse("2.1.0"));
    }

    #[test]
    fn build_metadata_is_ignored() {
        assert_eq!(parse("2.0.1+build.7"), parse("2.0.1"));
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "abc", "2.x.0", "2.0.1.2", "v", "-1.0.0"] {
            assert!(parse(bad).is_none(), "{bad} 不该被解析");
        }
    }

    #[test]
    fn compares_numerically_not_lexically() {
        assert!(is_newer("2.0.10", "2.0.9"));
        assert!(is_newer("2.1.0", "2.0.99"));
        assert!(is_newer("3.0.0", "2.99.99"));
        assert!(!is_newer("2.0.9", "2.0.10"));
    }

    #[test]
    fn equal_versions_are_not_newer() {
        assert!(!is_newer("2.0.0", "2.0.0"));
        assert!(!is_newer("v2.0.0", "2.0.0"));
        assert!(!is_newer("2.0", "2.0.0"));
    }

    #[test]
    fn release_outranks_prerelease_of_same_number() {
        assert!(is_newer("2.0.1", "2.0.1-beta.1"));
        assert!(!is_newer("2.0.1-beta.1", "2.0.1"));
    }

    #[test]
    fn prerelease_segments_follow_semver_rules() {
        assert!(is_newer("2.0.1-beta.2", "2.0.1-beta.1"));
        assert!(is_newer("2.0.1-beta.10", "2.0.1-beta.9"));
        // 数字标识低于字母标识。
        assert!(is_newer("2.0.1-alpha", "2.0.1-1"));
        assert!(is_newer("2.0.1-beta.1", "2.0.1-beta"));
    }

    #[test]
    fn unparseable_input_never_reports_an_update() {
        // 宁可漏报，也不要提示一个来路不明的版本号。
        assert!(!is_newer("latest", "2.0.0"));
        assert!(!is_newer("2.0.1", ""));
        assert!(!is_newer("", ""));
    }
}
