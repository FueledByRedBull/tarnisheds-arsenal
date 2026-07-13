from typing import Any


class SearchEstimate:
    weapon_candidates: int
    stat_candidates: int
    combinations: int


class GameData:
    def compatible_aow_names(self, weapon_name: str, affinity: str) -> list[str]: ...


def load_game_data(data_dir: str | None = None) -> GameData: ...
def estimate_search_space(**kwargs: Any) -> SearchEstimate: ...
def optimize_builds(**kwargs: Any) -> list[Any]: ...
