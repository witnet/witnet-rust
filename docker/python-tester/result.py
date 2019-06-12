#!/usr/bin/env python3

class Result(object):
    def and_then(self, b_function):
        return b_function(self.value) if isinstance(self, Ok) else self

    def get_or(self, default_function):
        return self.value if isinstance(self, Ok) else default_function(self)

    def map(self, map_function):
        return Ok(map_function(self.value)) if isinstance(self, Ok) else self

    def map_error(self, map_function):
        return Err(map_function(self.error)) if isinstance(self, Err) else self

    def or_else(self, b_function):
        return self if isinstance(self, Ok) else b_function(self.error)
    
    def unwrap(self):
        return self.value

    def unwrap_error(self):
        return self.error

    def inspect(self):
        print(self.__dict__)
        return self

class Ok(Result):
    def __init__(self, value):
        self.value = value

class Err(Result):
    def __init__(self, error):
        self.error = error