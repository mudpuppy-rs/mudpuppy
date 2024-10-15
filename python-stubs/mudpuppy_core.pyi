__all__ = ["mudpuppy_core", "MudpuppyCore", "Config"]

class Config: ...

class MudpuppyCore:
    async def config(self) -> Config:
        pass

mudpuppy_core: MudpuppyCore
"""
Cool dude! `Config` ?
"""
