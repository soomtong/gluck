// External scanner for Cyber: emits NEWLINE, INDENT and DEDENT tokens.
//
// Cyber blocks are delimited by indentation after a `:`. The scanner keeps a
// stack of indent widths. Line ends inside brackets are treated as whitespace
// because the grammar does not accept a NEWLINE there. Lines that hold only a
// `--` comment do not change the indentation.

#include "tree_sitter/array.h"
#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

enum TokenType {
    NEWLINE,
    INDENT,
    DEDENT,
    CLOSE_PAREN,
    CLOSE_BRACKET,
    CLOSE_BRACE,
    ERROR_SENTINEL,
};

typedef struct {
    Array(uint16_t) indents;
} Scanner;

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

void *tree_sitter_cyber_external_scanner_create(void) {
    Scanner *scanner = ts_calloc(1, sizeof(Scanner));
    array_init(&scanner->indents);
    array_push(&scanner->indents, 0);
    return scanner;
}

void tree_sitter_cyber_external_scanner_destroy(void *payload) {
    Scanner *scanner = (Scanner *)payload;
    array_delete(&scanner->indents);
    ts_free(scanner);
}

unsigned tree_sitter_cyber_external_scanner_serialize(void *payload, char *buffer) {
    Scanner *scanner = (Scanner *)payload;
    unsigned size = 0;
    // The first entry is always 0 and is not stored.
    for (uint32_t i = 1; i < scanner->indents.size; i++) {
        if (size + sizeof(uint16_t) > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) {
            break;
        }
        uint16_t value = *array_get(&scanner->indents, i);
        memcpy(&buffer[size], &value, sizeof(uint16_t));
        size += sizeof(uint16_t);
    }
    return size;
}

void tree_sitter_cyber_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    Scanner *scanner = (Scanner *)payload;
    array_clear(&scanner->indents);
    array_push(&scanner->indents, 0);
    for (unsigned i = 0; i + sizeof(uint16_t) <= length; i += sizeof(uint16_t)) {
        uint16_t value;
        memcpy(&value, &buffer[i], sizeof(uint16_t));
        array_push(&scanner->indents, value);
    }
}

bool tree_sitter_cyber_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    Scanner *scanner = (Scanner *)payload;

    // During error recovery every symbol is valid. Let the internal lexer run.
    if (valid_symbols[ERROR_SENTINEL]) {
        return false;
    }

    // The emitted token is zero width. Whitespace and comments that are read
    // below to find the next indentation are lexed again as extras.
    lexer->mark_end(lexer);

    bool found_end_of_line = false;
    uint32_t indent = 0;

    for (;;) {
        if (lexer->lookahead == '\n') {
            found_end_of_line = true;
            indent = 0;
            skip(lexer);
        } else if (lexer->lookahead == ' ') {
            indent++;
            skip(lexer);
        } else if (lexer->lookahead == '\t') {
            indent++;
            skip(lexer);
        } else if (lexer->lookahead == '\r' || lexer->lookahead == '\f') {
            indent = 0;
            skip(lexer);
        } else if (lexer->eof(lexer)) {
            indent = 0;
            found_end_of_line = true;
            break;
        } else if (lexer->lookahead == '-') {
            skip(lexer);
            if (lexer->lookahead != '-') {
                break;
            }
            // A `--` comment.
            if (!found_end_of_line) {
                // Trailing comment after code. The line end comes after it.
                return false;
            }
            while (lexer->lookahead != '\n' && !lexer->eof(lexer)) {
                skip(lexer);
            }
        } else {
            break;
        }
    }

    if (!found_end_of_line) {
        return false;
    }

    uint16_t current = *array_back(&scanner->indents);

    if (valid_symbols[INDENT] && indent > current) {
        array_push(&scanner->indents, (uint16_t)indent);
        lexer->result_symbol = INDENT;
        return true;
    }

    bool within_brackets = valid_symbols[CLOSE_PAREN] || valid_symbols[CLOSE_BRACKET] || valid_symbols[CLOSE_BRACE];

    if (indent < current && scanner->indents.size > 1 &&
        (valid_symbols[DEDENT] || (!valid_symbols[NEWLINE] && !within_brackets))) {
        array_pop(&scanner->indents);
        lexer->result_symbol = DEDENT;
        return true;
    }

    if (valid_symbols[NEWLINE]) {
        lexer->result_symbol = NEWLINE;
        return true;
    }

    return false;
}
