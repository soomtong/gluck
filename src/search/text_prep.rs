/// 식별자/Path 텍스트를 SimpleTokenizer가 단어 단위로 분해할 수 있게 전처리한다.
///
/// SimpleTokenizer는 `_`, `/`, `.`, `-` 등 비-alphanumeric 문자에서 자동 분해하지만
/// camelCase는 인식하지 못한다. 이 함수는 camelCase 경계(소문자 → 대문자, 글자 → 숫자)에
/// 공백을 삽입해서 `BuildIndex` → `Build Index`, `Rev2` → `Rev 2`로 만든다.
///
/// 한글 등 비-ASCII alphabet은 case 개념이 없어 변환되지 않음.
pub fn split_camel_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_lower_alpha = false;
    let mut prev_alpha = false;
    for c in s.chars() {
        let is_upper = c.is_ascii_uppercase();
        let is_lower = c.is_ascii_lowercase();
        let is_digit = c.is_ascii_digit();
        let is_alpha = is_upper || is_lower;
        if (is_upper && prev_lower_alpha) || (is_digit && prev_alpha) {
            out.push(' ');
        }
        out.push(c);
        prev_lower_alpha = is_lower;
        prev_alpha = is_alpha;
    }
    out
}

/// Path를 단어 후보로 만들기 위해 path separator를 공백으로 치환한 뒤
/// `split_camel_case`를 적용한다.
pub fn path_to_terms(path: &str) -> String {
    let replaced: String = path
        .chars()
        .map(|c| {
            if matches!(c, '/' | '.' | '-' | '_' | '\\') {
                ' '
            } else {
                c
            }
        })
        .collect();
    split_camel_case(&replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_unchanged_by_split() {
        assert_eq!(split_camel_case("rrf_fuse"), "rrf_fuse");
        assert_eq!(
            split_camel_case("build_index_incremental"),
            "build_index_incremental"
        );
    }

    #[test]
    fn camel_case_split() {
        assert_eq!(split_camel_case("BuildIndex"), "Build Index");
        assert_eq!(split_camel_case("ModalState"), "Modal State");
        assert_eq!(split_camel_case("HTTPServer"), "HTTPServer");
    }

    #[test]
    fn mixed_identifier() {
        assert_eq!(split_camel_case("buildIndexFor"), "build Index For");
    }

    #[test]
    fn path_terms_replaces_separators() {
        assert_eq!(path_to_terms("src/search/rrf.rs"), "src search rrf rs");
        assert_eq!(path_to_terms("src/git/store.rs"), "src git store rs");
    }

    #[test]
    fn path_terms_with_camel_case_file() {
        assert_eq!(
            path_to_terms("src/search/ModalState.rs"),
            "src search Modal State rs"
        );
    }

    #[test]
    fn empty_string() {
        assert_eq!(split_camel_case(""), "");
        assert_eq!(path_to_terms(""), "");
    }

    #[test]
    fn korean_passthrough() {
        assert_eq!(split_camel_case("한글이름"), "한글이름");
    }
}
