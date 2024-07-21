; Method definitions
(alias (identifier) @definition.function)
(setter (identifier) @definition.function)
(method name: [(identifier) (constant)] @definition.function)
(singleton_method name: [(identifier) (constant)] @definition.function)

; Class definitions
(
  (comment)* @doc
  .
  [
    (class
      name: [
        (constant) @name
        (scope_resolution
          name: (_) @name)
      ]) @definition.class
    (singleton_class
      value: [
        (constant) @name
        (scope_resolution
          name: (_) @name)
      ]) @definition.class
  ]
  (#strip! @doc "^#\\s*")
  (#select-adjacent! @doc @definition.class)
)

; Module definitions
(
  (module
    name: [
      (constant) @name
      (scope_resolution
        name: (_) @name)
    ]) @definition.module
)
