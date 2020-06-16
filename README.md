# tamago <img src="eggsushi.png" alt="egg sushi" height="40" align="left"/>
Re-implementation of the [TASO compiler](https://github.com/jiazhihao/TASO) 
using [equality saturation](https://mwillsey.com/papers/egg/). Tamago implements
both the graph transformation verifier and the optimizer; the former is complete while
the latter is in progress. 

## development
You need the TASO runtime (with its dependencies), rust and 
[rust-bindgen](https://github.com/rust-lang/rust-bindgen) to build tamago. The 
[`Dockerfile`](docker/Dockerfile) sets this all up for you. Execute `docker build --tag tamago:0.1 .`
to build the image, then run with `./run_docker.sh`. 

We recommend perusing the
`rust-bindgen` [guide](https://rust-lang.github.io/rust-bindgen/) and related 
docs, and note that its c++ support is primitive. 

## the verifier
The verifier re-implements TASO's [verify.py](https://github.com/jiazhihao/TASO/blob/master/verify/verify.py). 
It takes a list of 
transformation rules to be checked and populates an EGraph with the expressions from
these rules. Then it iteratively applies the axioms, checking if all rules are verified
after each round. If so it stops, indicating success; otherwise it continues until the 
EGraph saturates. If there are still un-verified rules after saturation, we can 
conclude those rules are unsound w.r.t. the axioms. This strategy is faster (~30x in
our simple experiments) than naively
verifying rule-by-rule, because the equality proofs of many rules may overlap, and each
EClass may contain expressions from many different rules. 

To run the verifier, uncomment `prove_taso_rules()` in `main.rs/main()`, comment out
`optimize()`, `cd` to project root and execute `cargo run --release taso_rules.txt`.
The `--release` flag turns on rust optimizations.

## the optimizer
The optimizer replaces TASO's [backtracking search](https://cs.stanford.edu/~padon/taso-sosp19.pdf)
with equality saturation. It directly 
uses the axioms as rewrite rules to drive the optimization, eliminating the need to
synthesize and verify a seperate set of transformation rules. It leverages TASO's
infrastructure for maintaining metadata like dimensions and strides, as well as TASO's cost
function that directly executes DL operators. Currently we adopt a simple greedy extraction
after saturation. However this extraction still achieves some degree of TASO's joint-optimization
of graph and layout. 

To run the optimizer, uncomment `optimize()` in `main.rs/main()`, comment out
`prove_taso_rules()`, `cd` to project root and execute `cargo run`. This will panic as the
optimzer implementation is in progress. 
