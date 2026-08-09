#ifndef FOREIGN_TYPES_H
#define FOREIGN_TYPES_H

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

// ── Dynamic array (Vec[T]) ────────────────────────────────────────────────────
//
// Usage:
//   vec_t v = vec_new(sizeof(int32_t));
//   int32_t x = 42;
//   vec_push(&v, &x);
//   int32_t *p = (int32_t *)vec_get(&v, 0);
//   vec_free(&v);

typedef struct {
    void   *data;
    size_t  len;
    size_t  cap;
    size_t  element_size;
} vec_t;

static inline vec_t vec_new(size_t element_size) {
    vec_t v;
    v.data         = NULL;
    v.len          = 0;
    v.cap          = 0;
    v.element_size = element_size;
    return v;
}

static inline void vec_push(vec_t *v, void *element) {
    if (v->len >= v->cap) {
        size_t new_cap = v->cap == 0 ? 8 : v->cap * 2;
        v->data        = realloc(v->data, new_cap * v->element_size);
        v->cap         = new_cap;
    }
    memcpy((char *)v->data + v->len * v->element_size, element, v->element_size);
    v->len++;
}

static inline void *vec_get(vec_t *v, size_t index) {
    return (char *)v->data + index * v->element_size;
}

static inline size_t vec_len(vec_t *v) {
    return v->len;
}

static inline void vec_free(vec_t *v) {
    free(v->data);
    v->data = NULL;
    v->len  = 0;
    v->cap  = 0;
}

// ── Hash map (HashMap[String, V] — string keys) ───────────────────────────────
//
// Note: map_t uses string keys only. If your Bullang source uses HashMap[i32, V]
// or any non-string key, cast the key to a string before calling map_set/map_get.
//
// Usage:
//   map_t m = map_new();
//   int32_t val = 99;
//   map_set(&m, "score", &val, sizeof(int32_t));
//   int32_t *p = (int32_t *)map_get(&m, "score");
//   map_free(&m);

#define MAP_INIT_CAP 16

typedef struct map_entry {
    char            *key;
    void            *value;
    size_t           value_size;
    struct map_entry *next;
} map_entry_t;

typedef struct {
    map_entry_t **buckets;
    size_t        cap;
    size_t        len;
} map_t;

static inline size_t map__hash(const char *key, size_t cap) {
    size_t h = 5381;
    while (*key) h = h * 33 ^ (unsigned char)*key++;
    return h % cap;
}

static inline map_t map_new(void) {
    map_t m;
    m.buckets = (map_entry_t **)calloc(MAP_INIT_CAP, sizeof(map_entry_t *));
    m.cap     = MAP_INIT_CAP;
    m.len     = 0;
    return m;
}

static inline void map_set(map_t *m, const char *key, void *value, size_t value_size) {
    size_t       idx   = map__hash(key, m->cap);
    map_entry_t *entry = m->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            free(entry->value);
            entry->value      = malloc(value_size);
            entry->value_size = value_size;
            memcpy(entry->value, value, value_size);
            return;
        }
        entry = entry->next;
    }
    map_entry_t *new_entry  = (map_entry_t *)malloc(sizeof(map_entry_t));
    size_t       key_len    = strlen(key) + 1;
    new_entry->key          = (char *)malloc(key_len);
    memcpy(new_entry->key, key, key_len);
    new_entry->value        = malloc(value_size);
    new_entry->value_size   = value_size;
    memcpy(new_entry->value, value, value_size);
    new_entry->next         = m->buckets[idx];
    m->buckets[idx]         = new_entry;
    m->len++;
}

static inline void *map_get(map_t *m, const char *key) {
    size_t       idx   = map__hash(key, m->cap);
    map_entry_t *entry = m->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) return entry->value;
        entry = entry->next;
    }
    return NULL;
}

static inline size_t map_len(map_t *m) {
    return m->len;
}

static inline void map_free(map_t *m) {
    for (size_t i = 0; i < m->cap; i++) {
        map_entry_t *entry = m->buckets[i];
        while (entry) {
            map_entry_t *next = entry->next;
            free(entry->key);
            free(entry->value);
            free(entry);
            entry = next;
        }
    }
    free(m->buckets);
    m->buckets = NULL;
    m->len     = 0;
    m->cap     = 0;
}

#endif /* FOREIGN_TYPES_H */
