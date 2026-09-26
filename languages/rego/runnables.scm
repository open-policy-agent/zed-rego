((
  (rule_head (var) @run @test_name
    (#match? @run "^test_"))
) @rego-test
(#set! tag rego-test)
)
