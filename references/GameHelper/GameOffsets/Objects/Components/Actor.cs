namespace GameOffsets.Objects.Components
{
    using GameOffsets.Natives;
    using System;
    using System.Runtime.InteropServices;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct ActorOffset
    {
        [FieldOffset(0x000)] public ComponentHeader Header;
        [FieldOffset(0x1B0)] public IntPtr CurrentActionPtr; // ActorActionStruct, only set while a skill is in use
        [FieldOffset(0x248)] public int AnimationId;
        [FieldOffset(0x6F0)] public StdVector ActiveSkillsPtr; // ActiveSkillStructure
        [FieldOffset(0x708)] public StdVector CooldownsPtr;
        [FieldOffset(0x720)] public StdVector VaalSoulsPtr;
        [FieldOffset(0x740)] public StdVector DeployedEntityArray; // DeployedEntityStructure
    }

    // exe 05.09.2026: the action the actor is performing right now (measured with Spark/Incinerate/Frostblink).
    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct ActorActionStruct
    {
        [FieldOffset(0xF0)] public IntPtr ActiveSkillPtr; // ActiveSkillDetails of the skill being used
        [FieldOffset(0x120)] public byte UseStage; // counts up while casting/channelling (Spark 0..2, Incinerate 1..8)
    }

    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct ActiveSkillStructure
    {
        // exe 05.09.2026: shared_ptr order is {object, control block} now (object used to be second).
        public IntPtr ActiveSkillPtr; // ActiveSkillsDetails
        public IntPtr UselessPtr0;
    }

    // exe 05.09.2026: 0x30 bytes were inserted before UnknownIdAndEquipmentInfo (incl. a weak self
    // pointer at 0x30), everything from there on moved by +0x30 (TotalUses 0xB4 confirmed: counts casts).
    // UseStage/CastType are NOT in this object anymore (nothing in 0x000..0x400 toggled while casting).
    // UseStage now comes from ActorActionStruct. CastType: no number field anymore - the kind of use is
    // the class of the current action object (Spark/Incinerate/Frostblink each had their own vtable).
    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct ActiveSkillDetails // based on heuristics this structure size is 0x100
    {
        //[FieldOffset(0x00)] public IntPtr UselessPtr0;
        [FieldOffset(0x08)] public int UseStage; // this is just a placeholder. Caller is suppose to update this one.
        [FieldOffset(0x0C)] public int CastType; // old offset, reads 0 since exe 05.09.2026 (see comment above).
        [FieldOffset(0x40)] public uint UnknownIdAndEquipmentInfo;
        [FieldOffset(0x48)] public IntPtr GrantedEffectsPerLevelDatRow;
        [FieldOffset(0x20)] public IntPtr ActiveSkillsDatPtr; // this is just a placeholder. Caller is suppose to update this one.
        [FieldOffset(0x28)] public int CurrentVaalSouls; // this is just a placeholder. Caller is suppose to update this one.
        [FieldOffset(0x60)] public IntPtr GrantedEffectStatSetsPerLevelDatRow;
        [FieldOffset(0xB0)] public bool CanBeUsedWithWeapon;
        [FieldOffset(0xB1)] public bool CannotBeUsed;
        [FieldOffset(0xB2)] public byte UnknownByte0;
        [FieldOffset(0xB3)] public byte UnknownByte1;
        [FieldOffset(0xB4)] public int TotalUses;
        //[FieldOffset(0x8C)] public int TotalCooldownTimeInMs;
        //[FieldOffset(0x9C)] public int SoulsPerUse;
        //[FieldOffset(0xA0)] public int TotalVaalUses;
        //[FieldOffset(0xB0)] public IntPtr StatsPtr;
        //[FieldOffset(0xB8)] public IntPtr UselessPtr3;
        //[FieldOffset(0xD8)] public StdVector LinkedSupportedGems; // according to poe.exe each support gem structure size is 0x80
    }

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct ActiveSkillCooldown
    {
        [FieldOffset(0x00)] public int Unknown0;
        [FieldOffset(0x04)] public int Unknown1;
        [FieldOffset(0x08)] public int ActiveSkillsDatId;
        [FieldOffset(0x0C)] public int Unknown2;
        [FieldOffset(0x10)] public StdVector CooldownsList;
        [FieldOffset(0x28)] public IntPtr PtrToActorComponentPlusOffset;
        [FieldOffset(0x30)] public int MaxUses;
        [FieldOffset(0x34)] public int TotalCooldownTimeInMs;
        //[FieldOffset(0x38)] public int ActiveSkillsDatId;
        [FieldOffset(0x3C)] public uint UnknownIdAndEquipmentInf0; // same as in ActiveSkillDetails.
        //[FieldOffset(0x40)] public int Unknown2;
        [FieldOffset(0x44)] public int PAD_0x44;

        public int TotalActiveCooldowns()
        {
            return (int)this.CooldownsList.TotalElements(0x10);
        }

        public bool CannotBeUsed()
        {
            return this.TotalActiveCooldowns() >= this.MaxUses;
        }
    }

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct VaalSoulStructure
    {
        [FieldOffset(0x00)] public IntPtr ActiveSkillsDatPtr;
        [FieldOffset(0x08)] public IntPtr UselessPtr;
        [FieldOffset(0x10)] public int RequiredSouls;
        [FieldOffset(0x14)] public int CurrentSouls;
        [FieldOffset(0x18)] public long PAD_0x18;

        public readonly bool CannotBeUsed()
        {
            return this.CurrentSouls < this.RequiredSouls;
        }
    }

    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    public struct DeployedEntityStructure
    {
        public int EntityId;
        public ushort ActiveSkillsDatId;
        public ushort PAD_0x0C;
        public ushort DeployedObjectType;
        public ushort PAD_0x014;

        public override readonly string ToString()
        {
            return $"{this.DeployedObjectType} - {this.ActiveSkillsDatId} - {this.EntityId}";
        }
    }
}