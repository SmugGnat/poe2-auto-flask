namespace GameOffsets.Objects.UiElement
{
    using System;
    using System.Runtime.InteropServices;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct SkillTreeNodeUiElementOffset
    {
        [FieldOffset(0x000)] public UiElementBaseOffset UiElementBase;
        [FieldOffset(0x298)] public IntPtr SkillInfo; // SkillInfoStruct (0x208 before exe 05.09.2026)
    }

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct SkillInfoStruct
    {
        [FieldOffset(0x08)] public IntPtr PassiveSkillsDatRow; // PassiveSkillsDatStruct
    }

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct PassiveSkillsDatStruct
    {
        // exe 05.09.2026: 16 bit; the (unaligned) Name string pointer follows at 0x32.
        [FieldOffset(0x30)] public ushort PassiveSkillGraphId;
    }
}
