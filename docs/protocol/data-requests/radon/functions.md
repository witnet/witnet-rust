# Predefined functions

## Filtering functions

- `gt`: must be greater than the provided value.
- `lt`: must be less than the provided value.
- `eq`: must equal the provided value.
- `dev-$type`: must not deviate from the average. This has three
  subtypes:
    - `dev-abs`: must not deviate from the average more than the 
    provided absolute value.
    - `dev-rel`: must not deviate from the average more than the 
    provided relative value (e.g.: `0.5` is 50%).
    - `dev-std`: must not deviate from the average more than `value`
      times the standard deviation of the values in the `Array`, where
      `value` is typically a `Float` between `1` (picky) and `3`
      (relaxed).
- `top`: must be amongst the `value` highest values in the `Array`.
- `bottom`: must be amongst the `value` lowest values in the `Array`.
- `not-$function`: applies the opposite of any of the previous functions (e.g.: `not-lt` equates to *"greater or equal
than"*).

!!! info ""
    The implicit signature for all of the filtering functions is:
    ```ts
    (value: T): Boolean
    ```

!!! warning ""
    Some filtering functions that compare individual values to the
    values in the `Array` are pointless if used along the `some` 
    operator as they will make it return `False` every time. 
    These include:
    
    - `top`
    - `bottom`

## Reducing functions

- `min`: takes the minimum value.
- `max`: takes the maximum value.
- `mode`: takes the [mode]. That is, the value that appears the more often.
- `avg-$type`: calculates the average of the values in the `Array`. 
This has four subtypes:
    - `avg-mean`: [arithmetic mean].
    - `avg-mean-w`: [weighted mean].
    - `avg-median`: [median].
    - `avg-median-w`: [weighted median].
- `dev-$type`: measures the dispersion of the values in the `Array`. 
This has four subtypes:
    - `dev-std`: [standard deviation].
    - `dev-avg`: [average absolute deviation].
    - `dev-med`: [median absolute deviation].
    - `dev-max`: [maximum absolute deviation].

## Hash functions

- BLAKE family:
    - `blake-256`
    - `blake-512`
    - `blake2s-256`
    - `blake2b-512`
- MD5: `md5-128`
- RIPEMD family:
    - `ripemd-128`
    - `ripemd-160`
    - `ripemd-320`
- SHA1: `sha1-160`
- SHA2 family:
    - `sha2-224`
    - `sha2-256`
    - `sha2-384`
    - `sha2-512`
- SHA3 family:
    - `sha3-224`
    - `sha3-256`
    - `sha3-384`
    - `sha3-512`
- Whirlpool: `whirlpool-512`

!!! warning "Safety of deprecated hash functions"
    The `md5-128` and `sha1-160` hash functions are provided solely for 
    the sake of backward compatibility with legacy software and systems.
    Depending on the use case, they may not live up to minimum 
    acceptable security standards. Please refrain from using those for 
    new software and systems unless strictly necessary.

[arithmetic mean]: https://en.wikipedia.org/wiki/Arithmetic_mean
[weighted mean]: https://en.wikipedia.org/wiki/Weighted_arithmetic_mean
[median]: https://en.wikipedia.org/wiki/Median
[weighted median]: https://en.wikipedia.org/wiki/Weighted_median
[mode]: https://en.wikipedia.org/wiki/Mode_(statistics)
[standard deviation]: https://en.wikipedia.org/wiki/Standard_deviation
[average absolute deviation]: https://en.wikipedia.org/wiki/Average_absolute_deviation
[median absolute deviation]: https://en.wikipedia.org/wiki/Median_absolute_deviation
[maximum absolute deviation]: https://en.wikipedia.org/wiki/Maximum_absolute_deviation