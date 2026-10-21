"""Public language hook dispatch over the shared Rust family query engine."""
import noon as m


def observe_query_hook(module, spec):
    import numpy as np

    events = []
    kind = spec["class"]
    children = [module.Circle(.2), module.Square(.3)] if kind in ("Group", "VGroup") else []
    root = getattr(module, kind)(*children)
    root.apply_over_attr_arrays(lambda _: np.asarray(
        [[0., 0., 0.], [.2, 0., 0.], [.2, .2, 0.], [0., .2, 0.]][:spec["ownPoints"]],
        dtype=float).reshape((-1, 3)))
    names = {id(root): "self", **{id(child): name for child, name in zip(children, ("a", "b"))}}
    hook = spec["hook"]

    def callback():
        events.append(hook)
        if hook.endswith("raises"):
            raise LookupError("query hook")
        if hook == "split_returns":
            return [root]
        if hook.startswith("get_num_points_"):
            return 0 if hook == "get_num_points_zero" else 7
        return hook.endswith("true")

    target = ("split" if hook.startswith("split_") else
              "get_num_points" if hook.startswith("get_num_points_") else "has_points")
    setattr(root, target, callback)
    try:
        operation = spec["operation"]
        if operation == "length":
            result = len(root)
        elif operation == "split":
            result = [names[id(value)] for value in root.split()]
        elif operation == "iteration":
            result = [names[id(value)] for value in root]
        elif operation == "index":
            result = names[id(root[0])]
        elif operation in ("has_points", "has_no_points", "get_num_points"):
            result = getattr(root, operation)()
        else:
            raise AssertionError(operation)
    except Exception as error:
        result = {"exception": type(error).__name__, "message": str(error)}
    return {"spec": spec, "events": events, "result": result}


class QueryHooks(m.Scene):
    async def construct(self):
        # The browser harness injects the pinned CE observations. Each phase
        # runs actual public Noon wrappers, cold and after session publication.
        for phase in ("authored", "live"):
            for case in QUERY_HOOK_CASES:
                observed = observe_query_hook(m, case["spec"])
                assert observed == case, (phase, case["spec"], observed, case)
            await self.wait(.25)
