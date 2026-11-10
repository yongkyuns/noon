# Documentation

Noon is an experimental animation engine with Rust and Python authoring. Start
with a runnable scene, learn the authoring model, then use the API and architecture
references when you need detail. These pages are built from the same checkout as
the website; the revision is shown below every page.

## Choose a starting point

| You want to… | Start here |
| --- | --- |
| Try an editable scene without installing anything | [Browser quick start](getting-started.html#browser) |
| Author in Python and run a native window | [Python walkthrough](python.html) |
| Write ordinary Rust functions and run them natively | [Rust walkthrough](rust.html) |
| Save PNG frames or an MP4 | [Exporting visuals](export.html) |
| Find a class, method, or parameter | [API reference](api.html) |
| Understand what the Rust engine owns | [Architecture](architecture.html) |

## Read the guides in order

After [getting started](getting-started.html), read the language walkthrough and
[scene authoring](guide.html). [Playback and interaction](playback.html) explains
why live input, callbacks, and finite replay have different capabilities.
[Examples](examples.html) pairs an interactive preview with editable source.

## Reference versus support

A symbol appearing in the API reference means it is exported in this checkout;
it does **not** by itself establish support in every host or every combination.
Native Rust, native CPython, and browser Python have different integration
boundaries. Consult the host notes and runnable examples. ManimCE semantics are a
useful point of reference for supported features, not a strict compatibility
promise or a long-term constraint.

Detailed references live on this website too: the
[implementation reference](reference.html), [browser integration](web-runtime.html),
[native Python details](native-python.html), [Rust library details](native-rust.html),
and [type map](internals.html) are rendered from their canonical repository sources.
