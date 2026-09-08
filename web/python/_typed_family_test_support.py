"""Typed family bridge doubles for CPython forwarding tests.

These model argument/results only. Rust integration and browser tests qualify
semantic validation, cycles, and publication; do not duplicate those engines here.
"""


def _identity(handle):
    value = handle.identity
    return value() if callable(value) else value


def _key(identity):
    if isinstance(identity, tuple):
        return f"{identity[-2]}:{identity[-1]}"
    return f"{identity}:0"


class FakeMembershipBatch:
    def __init__(self, kind):
        self.kind = kind
        self.members = []

    def appendMobject(self, wrapper_id, handle):
        assert wrapper_id == ""
        self.members.append(handle)

    def appendFamily(self, handle):
        self.members.append(handle)


def install_bridge(target, factory, family_class, object_class, *, js=False):
    for cls in (family_class, object_class):
        if not callable(getattr(cls, "identity", None)):
            # Some older snapshot fixtures use an integer identity in __init__.
            cls.semanticSlot = property(lambda self: int(_key(_identity(self)).split(":")[0]))
            cls.semanticGeneration = property(lambda self: int(_key(_identity(self)).split(":")[1]))

    def edit(self, batch):
        assert batch.kind in ("add", "remove")
        if getattr(self, "reject_membership", False):
            raise RuntimeError("shared family batch rejected")
        staged = list(self.members)
        changed = []
        for member in batch.members:
            identity = _identity(member)
            accepted = (identity not in staged) if batch.kind == "add" else (identity in staged)
            changed.append(accepted)
            if accepted:
                if batch.kind == "add":
                    staged.append(identity)
                else:
                    staged.remove(identity)
        self.members = staged
        return changed

    def create(batch):
        assert batch.kind == "add"
        family = factory()
        edit(family, batch)
        return family

    family_class.editMembership = edit
    family_class.memberKeys = lambda self: [_key(identity) for identity in self.members]
    setattr(target, "noonCreateAuthoringFamilyHandle" if js else "_create_family_handle", create)
    setattr(target, "noonAuthoringMembershipBatch" if js else "_new_membership_batch", FakeMembershipBatch)
