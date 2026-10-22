import struct
import unittest

from tools.phase1.extract_skill_animations import (
    ATTACK_BEHAVIOR_EVENT,
    BULLET_BEHAVIOR_EVENT,
    SP_EFFECT_EVENT,
    SkillHit,
    TaeEvent,
    behavior_attacks,
    behavior_param_id,
    build_attack_row,
    combo_routes,
    parse_category_script,
    read_tae,
    resolve_behavior,
    route_assignments,
    skill_anim_group,
    skill_hits,
)
from tools.phase1.extract_motion_workbook import ATTACK_PARAM_FIELDS

CATEGORY_SCRIPT = """
SWORD_ART_DIFF_CAT_DEFAULT = 0
SWORD_ART_DIFF_CAT_LARGE_WEAPON = 2
SWORD_ART_DIFF_CAT_POLEARM = 3
SwordArtsCategory = {
    [3] = {SWORD_ART_DIFF_CAT_DEFAULT, SWORD_ART_DIFF_CAT_LARGE_WEAPON,
           WEAPON_CATEGORY_TWINBLADE}, -- Spinning Slash
    [56] = {SWORD_ART_DIFF_CAT_DEFAULT, WEAPON_CATEGORY_FIST},
}
function GetSwordArtsDiffCategory(artID, idleCat, wepCat)
    local AowCat_blacklist = {
        [54]  = true,
    }
    local AowCat_whitelist = {
        [196] = true,
    }
    if AowCat_whitelist[artID] then
    elseif 170 <= artID and artID < 255 or artID >= 300 or AowCat_blacklist[artID] then
    end
    if artID == 169 then end
    if wepCat == WEAPON_CATEGORY_CLAW then
    elseif wep_cat1 == WEAPON_CATEGORY_LIGHT_LARGE_SWORD then end
    if wepCat == WEAPON_CATEGORY_LARGE_RAPIER then end
    if (artID == 90 or artID == 98 or artID == 151 or artID == 152) and x then end
    if artID >= 90 and artID <= 99 and y then end
end
"""
REQUEST_SCRIPT = """
        elseif env(GetSpEffectID, 100052) == TRUE then
            request = SWORDARTS_REQUEST_RIGHT_COMBO_1
        elseif env(GetSpEffectID, 100053) == TRUE then
            request = SWORDARTS_REQUEST_RIGHT_COMBO_2
    elseif request == SWORDARTS_REQUEST_RIGHT_NORMAL then
        if IsAttackSwordArts(c_SwordArtsID) == FALSE then
            is_find_atk = FALSE
        end
        if c_WeaponSwordArts == "60530000" then -- Jump Ash
            arts_idx = 1
        elseif arts_cat == SWORD_ART_DIFF_CAT_LARGE_WEAPON or arts_cat == WEAPON_CATEGORY_LARGE_KATANA then
            arts_idx = 1
        elseif arts_cat == WEAPON_CATEGORY_TWINBLADE then
            arts_idx = 4
        elseif arts_cat == WEAPON_CATEGORY_FIST or arts_cat
            == WEAPON_CATEGORY_BEAST_CLAW then
            arts_idx = 6
        elseif c_SwordArtsID == 312 and env(GetSpEffectID, 1878) == TRUE then
            arts_idx = 2
        end
        SetVariable("SwordArtsOneShotCategory", arts_idx)
"""


def tae_bytes(anims: dict[int, list[tuple[int, float, tuple[int, ...]]]]) -> bytes:
    """A minimal 64-bit TAE3 file: header, animation table, structs, events, params."""
    data = bytearray(0x60)
    data[0:8] = b"TAE \x00\x00\x00\xff"
    struct.pack_into("<iq", data, 0x54, len(anims), 0x60)
    table = len(data)
    data += bytes(16 * len(anims))
    for index, (anim_id, events) in enumerate(anims.items()):
        anim_offset = len(data)
        data += bytes(40)
        struct.pack_into("<qq", data, table + 16 * index, anim_id, anim_offset)
        headers = len(data)
        data += bytes(24 * len(events))
        struct.pack_into("<q", data, anim_offset, headers)
        struct.pack_into("<i", data, anim_offset + 32, len(events))
        for event_index, (event_type, start, values) in enumerate(events):
            time_offset = len(data)
            data += struct.pack("<ff", start, start + 0.1)
            params_offset = len(data)
            data += struct.pack(f"<{len(values)}i", *values)
            data_offset = len(data)
            data += struct.pack("<iiq", event_type, 0, params_offset)
            struct.pack_into(
                "<qqq", data, headers + 24 * event_index, time_offset, time_offset + 4, data_offset
            )
    return bytes(data)


class TaeReaderTests(unittest.TestCase):
    def test_reads_behavior_and_sp_effect_events_and_skips_others(self) -> None:
        data = tae_bytes(
            {
                30000: [(ATTACK_BEHAVIOR_EVENT, 0.1, (0, 0, 1))],
                40000: [
                    (ATTACK_BEHAVIOR_EVENT, 0.9, (0, 0, 3010)),
                    (0, 0.5, (72, 0, -1)),
                    (BULLET_BEHAVIOR_EVENT, 1.0, (10101, 0, 7754)),
                    (SP_EFFECT_EVENT, 1.2, (100052,)),
                ],
            }
        )
        anims = read_tae(data, first_anim_id=40000)
        self.assertEqual(list(anims), [40000])
        events = anims[40000]
        self.assertEqual([event.event_type for event in events], [1, 2, 66])
        self.assertEqual(events[0].values, (0, 0, 3010))
        self.assertEqual(events[2].values, (100052,))
        self.assertAlmostEqual(events[1].start, 1.0, places=6)

    def test_rejects_other_formats_truncation_and_duplicates(self) -> None:
        data = tae_bytes({40000: [(ATTACK_BEHAVIOR_EVENT, 0.1, (0, 0, 3010))]})
        with self.assertRaisesRegex(ValueError, "64-bit"):
            read_tae(b"TAE \x00\x00\x00\x00" + data[8:])
        with self.assertRaisesRegex(ValueError, "bounds"):
            read_tae(data[:0x70])
        duplicate = bytearray(tae_bytes({40000: [], 40001: []}))
        struct.pack_into("<q", duplicate, 0x70, 40000)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            read_tae(bytes(duplicate))


class JudgeChainTests(unittest.TestCase):
    def test_judge_maps_to_behavior_rows_through_the_variation_chain(self) -> None:
        self.assertEqual(behavior_param_id(3010, 0), 300_000_010)
        self.assertEqual(behavior_param_id(7754, 130), 700_130_754)
        # Judges below 1000 are weapon-attack behaviors.
        self.assertEqual(behavior_param_id(811, 5000), 105_000_811)
        behaviors = {300_000_010: {}, 301_800_441: {}, 301_812_010: {}}
        self.assertEqual(resolve_behavior(3010, 1812, behaviors), 301_812_010)
        self.assertEqual(resolve_behavior(3441, 1812, behaviors), 301_800_441)
        self.assertEqual(resolve_behavior(3010, 1700, behaviors), 300_000_010)
        self.assertIsNone(resolve_behavior(3441, 1700, behaviors))

    def test_behaviors_resolve_to_attacks_directly_or_along_the_bullet_chain(self) -> None:
        bullets = {
            1: {"atkId_Bullet": 500, "HitBulletID": 2, "intervalCreateBulletId": -1},
            2: {"atkId_Bullet": 501, "HitBulletID": 1, "intervalCreateBulletId": 9},
            3: {"atkId_Bullet": 0, "HitBulletID": 4, "intervalCreateBulletId": -1},
        }
        self.assertEqual(behavior_attacks({"refType": 0, "refId": 42}, bullets), ([(42, False)], []))
        self.assertEqual(
            behavior_attacks({"refType": 1, "refId": 1}, bullets),
            ([(500, True), (501, True)], ["interval_bullet_not_modeled"]),
        )
        self.assertEqual(behavior_attacks({"refType": 1, "refId": 3}, bullets), ([], ["missing_bullet"]))
        self.assertEqual(
            behavior_attacks({"refType": 2, "refId": 1}, bullets), ([], ["unsupported_behavior_ref_type_2"])
        )

    def test_skill_hits_number_hits_in_time_order_and_report_problems(self) -> None:
        behaviors = {
            300_000_010: {"refType": 0, "refId": 300_000_010, "stamina": 40},
            700_000_754: {"refType": 1, "refId": 1, "stamina": 0},
        }
        bullets = {1: {"atkId_Bullet": 200_033_958, "HitBulletID": -1, "intervalCreateBulletId": -1}}
        anims = {
            40000: (
                TaeEvent(BULLET_BEHAVIOR_EVENT, 1.0, (10101, 0, 7754)),
                TaeEvent(ATTACK_BEHAVIOR_EVENT, 0.5, (0, 0, 3010)),
                TaeEvent(ATTACK_BEHAVIOR_EVENT, 0.7, (0, 0, 3999)),
                TaeEvent(SP_EFFECT_EVENT, 0.1, (100052,)),
            )
        }
        hits, problems = skill_hits(anims, 0, behaviors, bullets)
        self.assertEqual(
            [(hit.hit_order, hit.attack_id, hit.is_bullet, hit.stamina) for hit in hits],
            [(1, 300_000_010, False, 40), (3, 200_033_958, True, 0)],
        )
        self.assertEqual(problems, [(40000, 3999, "unresolved_behavior")])


class CategoryScriptTests(unittest.TestCase):
    def test_parses_categories_and_maps_weapons_to_animation_groups(self) -> None:
        script = parse_category_script(CATEGORY_SCRIPT, REQUEST_SCRIPT)
        self.assertEqual(script.categories[3], frozenset({0, 2, 24}))
        self.assertEqual(script.blacklist, frozenset({54}))
        self.assertEqual(script.whitelist, frozenset({196}))
        # Weapon-ID and SpEffect branches do not choose a category's index.
        self.assertEqual(script.arts_index_by_category, {2: 1, 61: 1, 24: 4, 42: 6, 62: 6})
        self.assertEqual(skill_anim_group(script, 3, idle_cat=2, wep_cat=25), 2)
        self.assertEqual(skill_anim_group(script, 3, idle_cat=12, wep_cat=25), 2)
        self.assertEqual(skill_anim_group(script, 3, idle_cat=3, wep_cat=24), 24)
        self.assertEqual(skill_anim_group(script, 3, idle_cat=0, wep_cat=23), 0)
        # Claws borrow the Fist category; skills past 170 never vary.
        self.assertEqual(skill_anim_group(script, 56, idle_cat=0, wep_cat=22), 42)
        self.assertEqual(skill_anim_group(script, 200, idle_cat=2, wep_cat=25), 0)

    def test_changed_script_fails_closed(self) -> None:
        with self.assertRaisesRegex(ValueError, "script changed"):
            parse_category_script(CATEGORY_SCRIPT.replace("if artID == 169 then", ""), REQUEST_SCRIPT)
        with self.assertRaisesRegex(ValueError, "unknown hks category constant"):
            parse_category_script(
                CATEGORY_SCRIPT.replace("WEAPON_CATEGORY_TWINBLADE}", "WEAPON_CATEGORY_NEW}"),
                REQUEST_SCRIPT,
            )


class RouteTests(unittest.TestCase):
    def test_combo_effects_chain_follow_ups_and_lacking_fp_starts_nothing(self) -> None:
        anims = {
            40000: (TaeEvent(SP_EFFECT_EVENT, 1.0, (100052,)),),
            40005: (TaeEvent(SP_EFFECT_EVENT, 1.0, (100052,)),),
            40010: (TaeEvent(SP_EFFECT_EVENT, 1.0, (100053,)),),
            40020: (),
            40030: (TaeEvent(SP_EFFECT_EVENT, 1.0, (100053,)),),
            40035: (TaeEvent(SP_EFFECT_EVENT, 1.0, (100052,)),),
        }
        self.assertEqual(
            combo_routes(anims),
            [[40000], [40000, 40010], [40000, 40010, 40020], [40030], [40030, 40020]],
        )

    def test_assignments_cover_damaging_and_effect_rows_with_longest_route_first(self) -> None:
        anims = {40000: (TaeEvent(SP_EFFECT_EVENT, 1.0, (100052,)),), 40010: (), 40060: (), 40070: ()}
        rows = [
            {"sheet_row": "1", "sequence_variant": "a040000", "hit_order": "1", "is_damaging": "1"},
            {"sheet_row": "2", "sequence_variant": "a040010", "hit_order": "1", "is_damaging": "1"},
            {"sheet_row": "3", "sequence_variant": "a040060", "hit_order": "1", "is_damaging": "0"},
            {"sheet_row": "4", "sequence_variant": "a040070", "hit_order": "2", "is_damaging": "0"},
        ]
        assignments = route_assignments(7, "Skill", anims, rows, effect_rows={4})
        routes = {
            (row["route_id"], row["route_label"], row["route_priority"])
            for row in assignments
        }
        self.assertEqual(
            routes,
            {
                ("slots_00_10", "Activation + Follow-up 1", 0),
                ("slots_00", "Activation", 1),
                ("slots_70", "Animation slot 70", 2),
            },
        )
        follow_up = [row for row in assignments if row["route_id"] == "slots_00_10"]
        self.assertEqual(
            [(row["sheet_row"], row["action_id"], row["action_order"]) for row in follow_up],
            [(1, "slot_00", 1), (2, "slot_10", 2)],
        )


class AttackRowTests(unittest.TestCase):
    def test_attack_row_copies_atk_param_values_and_flags_variants(self) -> None:
        attack: dict[str, int | float] = {field: 0 for field in ATTACK_PARAM_FIELDS.values()}
        attack.update(atkPhysCorrection=150.0, atkSuperArmor=12.5, throwFlag=0, atkAttribute=253)
        attack["spEffectId0"] = -1
        hit = SkillHit(
            anim_id=42405, hit_order=2, judge_id=3011, behavior_id=300000011,
            attack_id=300000011, is_bullet=False, stamina=40,
        )
        row = build_attack_row(
            hit, sheet_row=9, aow_id=103, aow_name="Spinning Slash", variant="Twinblade", attack=attack
        )
        self.assertEqual(row["raw_name"], "[Twinblade] Spinning Slash a042405 [2] (Lacking FP)")
        self.assertEqual(
            (row["is_lacking_fp"], row["is_damaging"], row["physical_mv"], row["poise_base"]),
            ("1", "1", "150", "12.5"),
        )
        self.assertEqual(
            (row["physical_attack_attribute"], row["stamina_cost"], row["sp_effect_id0"]),
            ("adaptive_primary", "40", "-1"),
        )


if __name__ == "__main__":
    unittest.main()
