# `Map<K, T>` type

## `Map.entries()`
```ts
entries(): Array<Array<Mixed>>
```
```ts
MAP_ENTRIES
```
The `entries` operator returns an `Array<Array<Mixed>>` containing the keys and values from the input `Map<K, T>` as
`[ key, value ]: Array<Mixed>` pairs.

## `Map.get(key)`
```ts
get(key: K): T
```
```ts
[ MAP_GET, key ]
```
The `get` operator returns the `T` value or structure associated to the `key: K` from a `Map<K, T>`.

!!! danger ""
    This operator can throw a runtime exception if the supplied `key: K` cannot be found in the input `Map<K, T>`.
    Exceptions are handled as specified in the [Exception handling] section.

## `Map.keys()`
```ts
keys(): Array<K>
```
```ts
MAP_KEYS
```
The `keys` operator returns an `Array<K>` containing the keys of the input `Map<K, T>`.

## `Map.values()`
```ts
values(): Array<T>
```
```ts
MAP_VALUES
```
The `values` operator returns an `Array<T>` containing the values of the input `Map<K, T>`.