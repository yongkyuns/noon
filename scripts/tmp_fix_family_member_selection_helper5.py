from pathlib import Path

path = Path("scripts/tmp_shared_family_member_selection_migration.py")
text = path.read_text()
start = text.index("    new_target = '''")
end_marker = "'''\n    group_next_to = python.index"
end = text.index(end_marker, start)
new_target = r'''    new_target = '''    translation = None
    source_aligner_bounds = None
    if submobject_to_align is not None:
        source_aligner_bounds = _layout_bounds_handle(submobject_to_align)
    elif index_of_submobject_to_align is not None:
        source_aligner_bounds = _family_member_bounds_handle(
            self, int(index_of_submobject_to_align)
        )

    if source_aligner_bounds is not None:
        if _alignment_is_mobject(mobject_or_point):
            if index_of_submobject_to_align is not None:
                target_bounds = _family_member_bounds_handle(
                    mobject_or_point, int(index_of_submobject_to_align)
                )
            else:
                target_bounds = _layout_bounds_handle(mobject_or_point)
            if target_bounds is None or not hasattr(session, "nextToBoundsWithAligner"):
                return _ORIGINAL_GROUP_NEXT_TO(
                    self,
                    mobject_or_point,
                    direction,
                    buff,
                    aligned_edge=aligned_edge,
                    submobject_to_align=submobject_to_align,
                    index_of_submobject_to_align=index_of_submobject_to_align,
                    coor_mask=coor_mask,
                )
            translation = session.nextToBoundsWithAligner(
                source_aligner_bounds,
                target_bounds,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
        else:
            if not hasattr(session, "nextToPointWithAligner"):
                return _ORIGINAL_GROUP_NEXT_TO(
                    self,
                    mobject_or_point,
                    direction,
                    buff,
                    aligned_edge=aligned_edge,
                    submobject_to_align=submobject_to_align,
                    index_of_submobject_to_align=index_of_submobject_to_align,
                    coor_mask=coor_mask,
                )
            point = _base._as_vec2(mobject_or_point)
            translation = session.nextToPointWithAligner(
                source_aligner_bounds,
                point.x,
                point.y,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
    elif submobject_to_align is not None or index_of_submobject_to_align is not None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge=aligned_edge,
            submobject_to_align=submobject_to_align,
            index_of_submobject_to_align=index_of_submobject_to_align,
            coor_mask=coor_mask,
        )
    elif isinstance(mobject_or_point, _compat.Group):
        target_shared = _shared_family_layout_session(mobject_or_point)
        if target_shared is not None and hasattr(session, "nextToFamily"):
            translation = session.nextToFamily(
                target_shared[0],
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
    elif _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is not None and hasattr(session, "nextToMobject"):
            translation = session.nextToMobject(
                target_handle,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
    elif hasattr(session, "nextToPoint"):
        point = _base._as_vec2(mobject_or_point)
        translation = session.nextToPoint(
            point.x,
            point.y,
            vector.x,
            vector.y,
            float(buff),
            edge.x,
            edge.y,
            mask.x,
            mask.y,
        )
'''
'''
text = text[:start] + new_target + text[end + 4:]
old = '''    dispatch_start = python.index(
        "    if _alignment_is_mobject(mobject_or_point):\\n",
        group_next_to,
    )
'''
new = '''    dispatch_start = python.index("    translation = None\\n", group_next_to)
'''
if old not in text:
    raise RuntimeError("dispatch start block not found")
path.write_text(text.replace(old, new, 1))
