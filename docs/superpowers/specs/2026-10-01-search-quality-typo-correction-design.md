# 후보 3: typo 쿼리 교정 설계

상태: 구현·채택 (2026-10-01). 측정 결과는 `docs/plans/2026-10-01-search-quality-turbovec-1.0-plan.md`의 "후보 3" 절 참고.

## 문제

BM25의 title/path_terms는 단어 단위(SimpleTokenizer + LowerCaser)이고 body는 bigram(`ngram_2_2`)이다. 오타가 섞인 식별자(`is_binray_blob`, `build_index_incremntal`)는 title/path_terms에서 아무것도 맞히지 못하고, body bigram 부분 매칭만 약하게 남는다. 벡터 쪽도 오타 단어의 subword가 흩어져 정답을 놓친다. 기준선(report-10)의 typo 카테고리 MRR은 0.420으로 다른 영어 카테고리보다 낮다.

## 검토한 대안

- tantivy `FuzzyTermQuery`: 매칭 문서에 상수 점수(boost)만 주므로 IDF가 사라진다. 짧은 단어에서는 이웃 단어가 너무 많이 걸린다.
- 식별자 전용 n-gram 필드 추가: 정상 쿼리의 점수 분포까지 바뀌어 다른 카테고리에 회귀 위험이 있다.
- 쿼리 시점 철자 교정(채택): 어휘에 없는 단어만 손대므로 철자가 맞는 쿼리는 순위가 그대로다. 교정한 단어는 일반 BM25 term으로 점수가 매겨진다.

## 설계

1. 어휘: title, path_terms, 그리고 새 필드 `body_words`의 term dictionary. `body_words`는 본문에 `split_camel_case`를 적용해 단어 토크나이저로 색인하고 `IndexRecordOption::Basic`으로 둔다. 검색 쿼리에서는 쓰지 않는다. `SearchEngine::open`에서 한 번 읽어 `vocab: HashMap<term, doc_freq>`로 캐시한다(3글자 이상 ASCII 알파벳만). `INDEX_VERSION` 10.
2. 쿼리 단어: `path:"..."`를 뺀 쿼리를 camelCase/비영숫자 경계로 나누고 소문자로 바꾼 뒤, 4글자 이상 ASCII 알파벳 단어만 남긴다. 한국어 쿼리는 교정하지 않는다.
3. 교정 조건(`typo::correct`): 어휘에 없는 단어에 대해 다음을 모두 만족하는 term을 찾는다.
   - 첫 글자가 같다.
   - OSA 거리(인접 전치 = 1)가 정확히 1이다.
   - 서로 복수형 관계(`s`/`es` 접미사)가 아니다.
   - 8글자 미만 단어는 실제 타이핑 실수 유형만 허용한다: 글자 누락(`splt`→`split`), 인접 전치(`serach`→`search`), 같은 글자 중복(`fussion`→`fusion`). 글자 치환은 8글자 이상에서만 허용한다(`reciprical`→`reciprocal`).
   - 후보가 여럿이면 doc_freq가 높은 쪽, 그다음 사전순.
4. 적용(`typo_mode`, 기본 1): BM25 쿼리 끝에 교정 단어를 덧붙인다. 원래 단어는 그대로 둬서 body bigram 매칭을 유지한다. 임베딩 텍스트는 바꾸지 않는다. 모드 2(임베딩에도 덧붙임)와 모드 3(임베딩에서 치환)은 측정용으로 남겨 둔다.

## 버린 규칙과 이유

- 거리 2 허용(8글자 이상): 맞는 교정은 전부 거리 1이었고, 거리 2는 `checking`→`chunking`, `replication`→`replicating`, `downloads`→`downloader` 같은 다른 실제 단어만 만들었다.
- 짧은 단어의 치환: `boot`→`bool`, `game`→`gate`, `unity`→`unify`, `while`→`whale`. negative 쿼리 판별을 망친다.
- 복수형 교정: `injection`→`injections`가 negative 쿼리의 오답 순위를 7에서 3으로 끌어올렸다.
- title/path_terms만 어휘로 쓰기(633 term): `reciprocal`, `topological`, `binary`처럼 본문에만 나오는 단어를 교정하지 못하고, `while`처럼 흔한 단어를 모르는 단어로 취급해 오교정했다. `body_words`를 넣은 뒤 어휘는 2682 term이 됐다.

## 한계

- 3글자 이하 단어(`trm`, `dat`)는 교정하지 않는다.
- 교정이 맞아도 정답 문서에 그 단어가 없으면 효과가 없다. `reciprical rank fussion`의 정답 `src/search/rrf.rs` 본문에는 "reciprocal"이 없다.
- 이 저장소에 실제로 있는 오타 단어는 "알려진 단어"로 취급돼 교정되지 않는다. 그래서 평가 세트를 인용하는 문서는 `.glcignore`로 반드시 빼야 한다.
