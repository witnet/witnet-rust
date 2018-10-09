# Subscripts

Some operators like allow specifying particular sequences of calls (scripts) that will be executed inside the scope of
the operator itself. We name those as _subscripts_.

That is the case for the `Array<T>::map<0>(subscript: (item: T) => O)` operator, which applies a subscript in parallel
on every `T` item found in the input `Array<T>` and then collects the results of the `(item: T) => 0` subscripts into a
single `Array<O>` that commits to have the same number of items as the input `Array<T>`.

Therefore, the first call in a subscript must be compatible with the type of the input.