namespace GameOffsets.Objects.Components
{
    using System;
    using System.Runtime.InteropServices;
    using GameOffsets.Natives;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct TargetableOffsets
    {
        [FieldOffset(0x00)] public ComponentHeader Header;
        [FieldOffset(0x50)] public uint TargetableFlag; // exe 05.09.2026: +8, TODO verify with a non-targetable entity
        [FieldOffset(0x58)] public uint HiddenFlag;
    }

    public static class TargetableHelper
    {
        public static Func<uint, bool> IsTargetable = param => { return Util.isBitSetUint(param, 0); };
    }
}
