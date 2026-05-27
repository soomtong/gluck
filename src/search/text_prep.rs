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

/// 영문 path 토큰 → 한국어 별칭 사전. BM25 path_terms 필드에 주입해서
/// 한국어 쿼리(예: "검색 인덱스")가 영문 소스 path(예: src/search/indexer.rs)를
/// 잡을 수 있도록 표면을 만든다.
///
/// 핵심 원칙: path 토큰과 **정확히 일치**할 때만 매핑한다. substring 매칭은
/// 거짓 양성(false positive)을 만들기 쉬워 사용하지 않는다.
const KOREAN_ALIASES: &[(&str, &str)] = &[
    ("search", "검색"),
    ("index", "인덱스"),
    ("indexer", "인덱스 인덱서"),
    ("build", "빌드"),
    ("embed", "임베딩"),
    ("embedding", "임베딩"),
    ("vector", "벡터"),
    ("modal", "모달"),
    ("state", "상태"),
    ("theme", "테마"),
    ("commit", "커밋"),
    ("diff", "디프 비교"),
    ("revwalk", "순회 그래프"),
    ("walk", "순회"),
    ("graph", "그래프"),
    ("chunk", "청크 조각"),
    ("tokenizer", "토크나이저"),
    ("token", "토큰"),
    ("highlight", "하이라이트 강조"),
    ("engine", "엔진"),
    ("store", "저장소"),
    ("repo", "저장소"),
    ("cache", "캐시"),
    ("config", "설정"),
    ("pick", "선택"),
    ("view", "보기"),
    ("ui", "화면"),
    ("report", "리포트"),
    ("fixture", "픽스처"),
    ("rrf", "융합 합치"),
    ("fusion", "융합"),
    ("bm25", "비엠"),
    ("symbol", "심볼 기호"),
    ("metric", "지표"),
];

/// path에 들어 있는 영문 토큰을 글로서리로 lookup해서 공백 분리 한국어 문자열로 반환.
/// 매칭이 없으면 빈 문자열.
pub fn korean_aliases(path: &str) -> String {
    let normalized = path_to_terms(path).to_lowercase();
    let mut out = String::new();
    for token in normalized.split_whitespace() {
        for (en, ko) in KOREAN_ALIASES {
            if token == *en {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(ko);
                break;
            }
        }
    }
    out
}

/// 쿼리에 Hangul Syllables 블록(U+AC00–U+D7AF) 글자가 하나라도 있으면 한국어 쿼리로 본다.
/// 비율 기반이 아니라 존재 기반: "검색 modal" 같은 혼합도 한국어 쿼리로 판정해
/// vector 기여를 가산한다.
pub fn is_korean_query(query: &str) -> bool {
    query.chars().any(|c| matches!(c, '\u{AC00}'..='\u{D7AF}'))
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

    #[test]
    fn korean_aliases_matches_path_tokens() {
        assert_eq!(
            korean_aliases("src/search/indexer.rs"),
            "검색 인덱스 인덱서"
        );
        assert_eq!(
            korean_aliases("src/search/modal_state.rs"),
            "검색 모달 상태"
        );
        assert_eq!(
            korean_aliases("src/highlight/engine.rs"),
            "하이라이트 강조 엔진"
        );
        assert_eq!(korean_aliases("src/git/commit.rs"), "커밋");
    }

    #[test]
    fn korean_aliases_returns_empty_for_unmapped_path() {
        assert_eq!(korean_aliases("src/foo/bar.rs"), "");
        assert_eq!(korean_aliases(""), "");
    }

    #[test]
    fn korean_aliases_lowercases_camelcase() {
        // path_to_terms가 ModalState를 "Modal State"로 분리하고, 우리는 lowercase 후 매칭.
        assert_eq!(
            korean_aliases("src/search/ModalState.rs"),
            "검색 모달 상태"
        );
    }

    #[test]
    fn is_korean_query_detects_hangul() {
        assert!(is_korean_query("검색 인덱스"));
        assert!(is_korean_query("검색 modal state"));
        assert!(!is_korean_query("search index"));
        assert!(!is_korean_query(""));
        assert!(!is_korean_query("123 _-/"));
    }
}
