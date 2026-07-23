#ifndef TUCKDB_H
#define TUCKDB_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>

#if defined(_WIN32) || defined(_WIN64)
#define TUCKDB_API __declspec(dllexport)
#else
#define TUCKDB_API __attribute__((visibility("default")))
#endif

/* Opaque handle types */
typedef struct tuckdb_table tuckdb_table_t;
typedef struct tuckdb_result tuckdb_result_t;

/* Create a new table with the given schema JSON.
 * schema_json format: [{"name":"col","type":"Int64"}, ...]
 * Supported types: Int64, Float64, Utf8, Timestamp
 * Returns NULL on failure. Call tuckdb_error_message() for details. */
TUCKDB_API tuckdb_table_t* tuckdb_create(const char* name, const char* schema_json, const char* path);

/* Open an existing table from a path.
 * Returns NULL on failure. */
TUCKDB_API tuckdb_table_t* tuckdb_open(const char* name, const char* path);

/* Insert a row into the table.
 * int_col_names / float_col_names / str_col_names are comma-separated column name lists.
 * int_cols / float_cols / str_cols are parallel arrays of values.
 * Returns 0 on success, -1 on error. */
TUCKDB_API int tuckdb_insert(tuckdb_table_t* table, int num_columns,
                             const int64_t* int_cols, const char* int_col_names,
                             const double* float_cols, const char* float_col_names,
                             const char* const* str_cols, const char* str_col_names);

/* Execute a SQL query.
 * Supported: SELECT col1, col2 FROM table WHERE condition
 * Returns NULL on failure. */
TUCKDB_API tuckdb_result_t* tuckdb_query(tuckdb_table_t* table, const char* sql);

/* Result accessors */
TUCKDB_API int tuckdb_result_num_rows(tuckdb_result_t* result);
TUCKDB_API int tuckdb_result_num_cols(tuckdb_result_t* result);
TUCKDB_API const char* tuckdb_result_column_name(tuckdb_result_t* result, int col);
TUCKDB_API double tuckdb_result_value_double(tuckdb_result_t* result, int row, int col);
TUCKDB_API const char* tuckdb_result_value_string(tuckdb_result_t* result, int row, int col);
TUCKDB_API int64_t tuckdb_result_value_int(tuckdb_result_t* result, int row, int col);

/* Free resources */
TUCKDB_API void tuckdb_table_free(tuckdb_table_t* table);
TUCKDB_API void tuckdb_result_free(tuckdb_result_t* result);

/* Get the last error message. The returned string is valid until
 * the next call to any tuckdb_* function. */
TUCKDB_API const char* tuckdb_error_message(int code);

#ifdef __cplusplus
}
#endif

#endif /* TUCKDB_H */
