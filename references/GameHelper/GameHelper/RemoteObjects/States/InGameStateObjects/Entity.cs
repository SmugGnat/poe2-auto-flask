// <copyright file="Entity.cs" company="None">
// Copyright (c) None. All rights reserved.
// </copyright>

namespace GameHelper.RemoteObjects.States.InGameStateObjects
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Linq;
    using Components;
    using GameHelper.RemoteEnums;
    using GameHelper.RemoteEnums.Entity;
    using GameOffsets.Objects.States.InGameState;
    using ImGuiNET;
    using Utils;

    /// <summary>
    ///     Points to an Entity/Object in the game.
    ///     Entity is basically item/monster/effect/player/etc on the ground.
    /// </summary>
    public class Entity : RemoteObjectBase
    {
        private static readonly int MaxComponentsInAnEntity = 50;
        private static readonly string DeliriumHiddenMonsterStarting =
            "Metadata/Monsters/LeagueAffliction/DoodadDaemons/DoodadDaemon";
        private static readonly string DeliriumUselessMonsterStarting =
            "Metadata/Monsters/LeagueAffliction/Volatile/";
        private static readonly string LegionLeagueMonsterStarting =
            "Metadata/Monsters/LegionLeague/";
        private static readonly string HarbingerLeagueMonsterStarting =
            "Metadata/Monsters/Avatar/";
        private static readonly string TormentedSpirit =
            "Metadata/Monsters/Spirit/Tormented";
        private static readonly string BetrayalLeagueMonsterStarting =
            "Metadata/Monsters/LeagueBetrayal/Betrayal";
        private static readonly string AzmeriMiscellaneousObjectStarting =
            "Metadata/MiscellaneousObjects/Azmeri/";
        private static readonly string AbyssMiscellaneousObjectStarting =
            "Metadata/MiscellaneousObjects/Abyss/Abyss";

        private static readonly string RitualUselessMonster =
            "Metadata/Monsters/LeagueAzmeri/TendrilSentinelVolatile";
        private static readonly string SirusUselessMonster =
            "Metadata/Monsters/AtlasExiles/OrionArenaObjects/OriathCivilian";

        private static readonly List<string> UselessWalls = new()
        {
            "Metadata/Monsters/AtlasExiles/OrionArenaObjects/OrionBlockingWall", // Walls for meteor and corridor attacks
        };

        private static readonly List<string> ArchnemesisUselessMonster = new()
        {
            "Metadata/Monsters/LeagueArchnemesis/LivingCrystal",
            "Metadata/Monsters/VolatileCore/VolatileCoreArchnemesis", // Exploding orbs / volatiles
            "Metadata/Monsters/LeagueArchnemesis/ToxicVolatile", // Caustic orbs on death
            "Metadata/Monsters/SummonedPhantasm/SummonedPhantasmIncursionExplode",
        };

        private static readonly List<string> MavenUselessMonsters = new()
        {
            "Metadata/Monsters/InvisibleFire/MavenLaserBarrageTarget",
            "Metadata/Monsters/MavenBoss/MavenBrainOrbitDaemon",
            "Metadata/Monsters/MavenBoss/MavenBrainVoidsandDaemon",
            "Metadata/Monsters/MavenBoss/TheMavenMap",
            "Metadata/Monsters/MavenBoss/TheMavenProving",
        };

        private static readonly List<string> Tier17UselessMonsters = new()
        {
            "Metadata/Monsters/Daemon/DaemonElderTentacle",
            "Metadata/Monsters/AtlasBosses/TheShaperBossProjectiles",
            "Metadata/Monsters/AtlasExiles/AtlasExile5Wild",
            "Metadata/Monsters/AtlasExiles/AtlasExile5Apparition",
            "Metadata/Monsters/AtlasExiles/AtlasExile4Apparition",
            "Metadata/Monsters/AtlasExiles/AtlasExile3Apparition",
            "Metadata/Monsters/AtlasExiles/AtlasExile2Apparition",
            "Metadata/Monsters/AtlasExiles/AtlasExile1Apparition",
        };

        private static readonly List<string> CortexUselessMonsters = new()
        {
            "Metadata/Monsters/LeagueSynthesis/SynthesisVenariusBoss", // Venarius (NPC) who spawns the actual bosses we fight in cortex
            "Metadata/Monsters/VolatileCore/SynthesisWormholeVolatile", // Volatiles from teleport slam ("feel the pull of the void")
        };

        private readonly ConcurrentDictionary<string, IntPtr> componentAddresses;
        private readonly ConcurrentDictionary<string, ComponentBase> componentCache;
        private NearbyZones zone;
        private int customGroup;
        private EntitySubtypes oldSubtypeWithoutPOI;

        /// <summary>
        ///     Initializes a new instance of the <see cref="Entity" /> class.
        /// </summary>
        /// <param name="address">address of the Entity.</param>
        internal Entity(IntPtr address)
            : this()
        {
            this.Address = address;
        }

        /// <summary>
        ///     Initializes a new instance of the <see cref="Entity" /> class.
        ///     NOTE: Without providing an address, only invalid and empty entity is created.
        /// </summary>
        internal Entity()
            : base(IntPtr.Zero, true)
        {
            this.componentAddresses = new();
            this.componentCache = new();
            this.zone = NearbyZones.None;
            this.Path = string.Empty;
            this.Id = 0;
            this.IsValid = false;
            this.EntityType = EntityTypes.Unidentified;
            this.EntitySubtype = EntitySubtypes.Unidentified;
            this.oldSubtypeWithoutPOI = EntitySubtypes.None;
            this.EntityState = EntityStates.None;
            this.customGroup = 0;
        }

        /// <summary>
        ///     Gets the Path (e.g. Metadata/Character/int/int) assocaited to the entity.
        /// </summary>
        public string Path { get; private set; }

        /// <summary>
        ///     Gets the Id associated to the entity. This is unique per map/Area.
        /// </summary>
        public uint Id { get; private set; }

        /// <summary>
        ///     Gets a value indicating whether the entity is near the player or not.
        ///     Useless and Invalid entities are not considered nearby.
        /// </summary>
        public NearbyZones Zones => this.IsValid ? this.zone : NearbyZones.None;

        /// <summary>
        ///     Gets or Sets a value indicating whether the entity
        ///     exists in the game or not.
        /// </summary>
        public bool IsValid { get; set; }

        /// <summary>
        ///     Gets a value indicating the type of entity this is. This never changes
        ///     for a given entity until entity is removed from GH memory (i.e. area change)
        /// </summary>
        public EntityTypes EntityType { get; protected set; }

        /// <summary>
        ///     Get a value indicating the sub-type of the entity. This never changes
        ///     for a given entity until entity is removed from GH memory (i.e. area change)
        /// </summary>
        public EntitySubtypes EntitySubtype { get; protected set; }

        /// <summary>
        ///     Gets the custom group given to a <see cref="EntityTypes.Monster"/> entity type by the user.
        /// </summary>
        public int EntityCustomGroup => this.EntitySubtype == EntitySubtypes.POIMonster ? this.customGroup : 0;

        /// <summary>
        ///     Get a value indicating the state the entity is in. This can change on every frame.
        /// </summary>
        public EntityStates EntityState { get; protected set; }

        /// <summary>
        ///     Gets a value indicating whether this entity can be exploded (or
        ///     removed from the game memory) while it's inside the network bubble.
        /// </summary>
        public bool CanExplodeOrRemovedFromGame => this.EntityState == EntityStates.Useless ||
            this.EntityType == EntityTypes.Renderable ||
            this.EntityType == EntityTypes.ImportantMiscellaneousObject ||
            this.EntityType == EntityTypes.DeliriumSpawner ||
            this.EntityType == EntityTypes.DeliriumBomb ||
            (this.EntityType == EntityTypes.Monster &&
            this.EntityState != EntityStates.LegionStage1Dead);  // They are not really dead yet.

        /// <summary>
        ///     Calculate the distance from the other entity.
        /// </summary>
        /// <param name="other">Other entity object.</param>
        /// <returns>
        ///     the distance from the other entity
        ///     if it can calculate; otherwise, return 0.
        /// </returns>
        public int DistanceFrom(Entity other)
        {
            if (this.TryGetComponent<Render>(out var myPosComp) &&
                other.TryGetComponent<Render>(out var otherPosComp))
            {
                var dx = myPosComp.GridPosition.X - otherPosComp.GridPosition.X;
                var dy = myPosComp.GridPosition.Y - otherPosComp.GridPosition.Y;
                return (int)Math.Sqrt(dx * dx + dy * dy);
            }

            return 0;
        }

        /// <summary>
        ///     Gets the Component data associated with the entity.
        /// </summary>
        /// <typeparam name="T">Component type to get.</typeparam>
        /// <param name="component">component data.</param>
        /// <param name="shouldCache">should entity cache this component or not.</param>
        /// <returns>true if the entity contains the component; otherwise, false.</returns>
        public bool TryGetComponent<T>(out T component, bool shouldCache = true)
            where T : ComponentBase
        {
            component = null;
            var componenName = typeof(T).Name;
            if (this.componentCache.TryGetValue(componenName, out var comp))
            {
                component = (T)comp;
                return true;
            }

            if (this.componentAddresses.TryGetValue(componenName, out var compAddr))
            {
                if (compAddr != IntPtr.Zero)
                {
                    component = Activator.CreateInstance(typeof(T), compAddr) as T;
                    if (component != null)
                    {
                        if (shouldCache)
                        {
                            this.componentCache[componenName] = component;
                        }

                        return true;
                    }
                }
            }

            return false;
        }

        /// <summary>
        ///     Validates if the monster is/was of a specific subtype.
        /// </summary>
        /// <param name="subType">subType the entity is/was in.</param>
        /// <returns></returns>
        public bool IsOrWasMonsterSubType(EntitySubtypes subType)
        {
            var toCheck = this.EntitySubtype == EntitySubtypes.POIMonster ?
                this.oldSubtypeWithoutPOI : this.EntitySubtype;
            return toCheck == subType;
        }

        /// <summary>
        ///     Converts the <see cref="Entity" /> class data to ImGui.
        /// </summary>
        internal override void ToImGui()
        {
            base.ToImGui();
            ImGui.Text($"Path: {this.Path}");
            ImGui.Text($"Id: {this.Id}");
            ImGui.Text($"Is Valid: {this.IsValid}");
            ImGui.Text($"Nearby Zone: {this.Zones}");
            ImGui.Text($"Entity Type: {this.EntityType}");
            ImGui.Text($"Entity SubType: {this.EntitySubtype}");
            if (this.EntitySubtype == EntitySubtypes.POIMonster)
            {
                ImGui.Text($"Entity Old SubType: {this.oldSubtypeWithoutPOI}");
            }

            ImGui.Text($"Entity Custom Group: {this.EntityCustomGroup}");
            ImGui.Text($"Entity State: {this.EntityState}");
            if (ImGui.TreeNode("Components"))
            {
                foreach (var kv in this.componentAddresses)
                {
                    if (this.componentCache.TryGetValue(kv.Key, out var value))
                    {
                        if (ImGui.TreeNode($"{kv.Key}"))
                        {
                            value.ToImGui();
                            ImGui.TreePop();
                        }
                    }
                    else
                    {
                        var componentType = Type.GetType($"{typeof(NPC).Namespace}.{kv.Key}");
                        if (componentType != null)
                        {
                            if (ImGui.SmallButton($"Load##{kv.Key}"))
                            {
                                this.LoadComponent(componentType);
                            }

                            ImGui.SameLine();
                        }

                        ImGuiHelper.IntPtrToImGui(kv.Key, kv.Value);
                    }
                }

                ImGui.TreePop();
            }
        }

        internal IEnumerable<string> GetComponentNames()
        {
            return this.componentAddresses.Keys;
        }

        internal void UpdateNearby(Entity player)
        {
            if (this.EntityState != EntityStates.Useless)
            {
                var distance = this.DistanceFrom(player);
                if (distance < Core.GHSettings.InnerCircle.Meaning)
                {
                    this.zone = NearbyZones.InnerCircle | NearbyZones.OuterCircle;
                    return;
                }
                else if (distance < Core.GHSettings.OuterCircle.Meaning)
                {
                    this.zone = NearbyZones.OuterCircle;
                    return;
                }
            }

            this.zone = NearbyZones.None;
        }

        /// <summary>
        ///     Updates the component data associated with the Entity base object (i.e. item).
        /// </summary>
        /// <param name="idata">Entity base (i.e. item) data.</param>
        /// <param name="hasAddressChanged">has this class Address changed or not.</param>
        /// <returns> false if this method detects an issue, otherwise true</returns>
        protected bool UpdateComponentData(ItemStruct idata, bool hasAddressChanged)
        {
            var reader = Core.Process.Handle;
            if (hasAddressChanged)
            {
                this.componentAddresses.Clear();
                this.componentCache.Clear();
                var entityDetails = reader.ReadMemory<EntityDetails>(idata.EntityDetailsPtr);
                this.Path = reader.ReadStdWString(entityDetails.name);
                if (string.IsNullOrEmpty(this.Path))
                {
                    return false;
                }

                var lookupPtr = reader.ReadMemory<ComponentLookUpStruct>(
                    entityDetails.ComponentLookUpPtr);
                if (lookupPtr.ComponentsNameAndIndex.Capacity > MaxComponentsInAnEntity)
                {
                    return false;
                }

                var namesAndIndexes = reader.ReadStdBucket<ComponentNameAndIndexStruct>(
                    lookupPtr.ComponentsNameAndIndex);
                var entityComponent = reader.ReadStdVector<IntPtr>(idata.ComponentListPtr);
                for (var i = 0; i < namesAndIndexes.Length; i++)
                {
                    var nameAndIndex = namesAndIndexes[i];
                    if (nameAndIndex.Index >= 0 && nameAndIndex.Index < entityComponent.Length)
                    {
                        var name = reader.ReadString(nameAndIndex.NamePtr);
                        if (!string.IsNullOrEmpty(name))
                        {
                            this.componentAddresses.TryAdd(name, entityComponent[nameAndIndex.Index]);
                        }
                    }
                }
            }
            else
            {
                foreach (var kv in this.componentCache)
                {
                    kv.Value.Address = kv.Value.Address;
                    if (!kv.Value.IsParentValid(this.Address))
                    {
                        return false;
                    }
                }
            }

            return true;
        }

        /// <inheritdoc />
        protected override void CleanUpData()
        {
            this.componentAddresses?.Clear();
            this.componentCache?.Clear();
        }

        /// <inheritdoc />
        protected override void UpdateData(bool hasAddressChanged)
        {
            var reader = Core.Process.Handle;
            var entityData = reader.ReadMemory<EntityOffsets>(this.Address);
            this.IsValid = EntityHelper.IsValidEntity(entityData.IsValid);
            if (!this.IsValid)
            {
                // Invalid entity data is normally corrupted. let's not parse it.
                return;
            }

            this.Id = entityData.Id;
            if (this.EntityState == EntityStates.Useless)
            {
                // let's not read or parse any useless entity components.
                return;
            }

            if (!this.UpdateComponentData(entityData.ItemBase, hasAddressChanged))
            {
                this.UpdateComponentData(entityData.ItemBase, true);
            }

            if (this.EntityType == EntityTypes.Unidentified)
            {
                if (!this.TryCalculateEntityType())
                {
                    this.EntityState = EntityStates.Useless;
                    return;
                }
            }

            if (this.EntitySubtype == EntitySubtypes.Unidentified)
            {
                if (!this.TryCalculateEntitySubType())
                {
                    this.EntityState = EntityStates.Useless;
                    return;
                }
            }

            this.CalculateEntityState();
        }

        /// <summary>
        ///     Loads the component class for the entity.
        /// </summary>
        /// <param name="componentType"></param>
        /// <exception cref="NotImplementedException"></exception>
        private void LoadComponent(Type componentType)
        {
            if (this.componentAddresses.TryGetValue(componentType.Name, out var compAddr))
            {
                if (compAddr != IntPtr.Zero)
                {
                    if (Activator.CreateInstance(componentType, compAddr) is ComponentBase component)
                    {
                        this.componentCache[componentType.Name] = component;
                    }
                }
            }
        }

        /// <summary>
        ///     Try to figure out the entity type of an entity.
        /// </summary>
        /// <returns>return false if it fails to do so, otherwise true.</returns>
        private bool TryCalculateEntityType()
        {
            if (!this.TryGetComponent<Render>(out var _))
            {
                return false;
            }
            else if (this.TryGetComponent<Chest>(out var _))
            {
                this.EntityType = EntityTypes.Chest;
            }
            else if (this.TryGetComponent<Player>(out var _))
            {
                this.EntityType = EntityTypes.Player;
            }
            else if (this.TryGetComponent<Shrine>(out var _))
            {
                // NOTE: Do not send Shrine to useless because it can go back to not used.
                //       e.g. Shrine in PVP area can do that.
                this.EntityType = EntityTypes.Shrine;
            }
            else if (this.TryGetComponent<Life>(out var _))
            {
                if (this.TryGetComponent<TriggerableBlockage>(out var _) &&
                    (!UselessWalls.Any(this.Path.StartsWith)))
                {
                    this.EntityType = EntityTypes.Blockage;
                }
                else if (!this.TryGetComponent<Positioned>(out var pos))
                {
                    return false;
                }
                else if (!this.TryGetComponent<ObjectMagicProperties>(out var _))
                {
                    return false;
                }
                else if (!pos.IsFriendly && this.TryGetComponent<DiesAfterTime>(out var _))
                {
                    if (this.TryGetComponent<Targetable>(out var tComp) && tComp.IsTargetable)
                    {
                        this.EntityType = EntityTypes.Monster;
                    }
                    else
                    {
                        return false;
                    }
                }
                else if (this.Path.StartsWith(DeliriumHiddenMonsterStarting) ||
                    this.Path.StartsWith(DeliriumUselessMonsterStarting)) // Delirium non-monster entity detector
                {
                    if (this.Path.Contains("BloodBag"))
                    {
                        this.EntityType = EntityTypes.DeliriumBomb;
                    }
                    else if (this.Path.Contains("EggFodder"))
                    {
                        this.EntityType = EntityTypes.DeliriumSpawner;
                    }
                    else if (this.Path.Contains("GlobSpawn"))
                    {
                        this.EntityType = EntityTypes.DeliriumSpawner;
                    }
                    else
                    {
                        return false;
                    }
                }
                else if (this.componentAddresses.ContainsKey("Buffs"))
                {
                    this.EntityType = EntityTypes.Monster;
                }
                else
                {
                    return false;
                }
            }
            else if (this.TryGetComponent<NPC>(out var _))
            {
                this.EntityType = EntityTypes.NPC;
            }
            else if (this.Path.StartsWith(AzmeriMiscellaneousObjectStarting))
            {
                if (!this.TryGetComponent<Targetable>(out var _))
                {
                    this.EntityType = EntityTypes.ImportantMiscellaneousObject;
                }
                else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriDustConverter"))
                {
                    this.EntityType = EntityTypes.ImportantMiscellaneousObject;
                    this.EntitySubtype = EntitySubtypes.AzmeriDustConvertor;
                }
                else
                {
                    this.EntityType = EntityTypes.Shrine;
                }
            }
            else if (this.Path.StartsWith(AbyssMiscellaneousObjectStarting))
            {
                this.EntityType = EntityTypes.ImportantMiscellaneousObject;
            }
            else if (Core.GHSettings.ProcessAllRenderableEntities
                && this.TryGetComponent<Positioned>(out var _))
            {
                this.EntityType = EntityTypes.Renderable;
            }
            else
            {
                return false;
            }

            return true;
        }

        /// <summary>
        ///     Try to figure out the sub-type of an entity.
        /// </summary>
        /// <returns>false if it fails to do so, otherwise true.</returns>
        private bool TryCalculateEntitySubType()
        {
            switch (this.EntityType)
            {
                case EntityTypes.Unidentified:
                    throw new Exception($"Entity with path ({this.Path}) and Id (${this.Id}) is unidentified.");
                case EntityTypes.Chest:
                    this.TryGetComponent<Chest>(out var chestComp);
                    if (this.Path.StartsWith("Metadata/Chests/LeagueAzmeri/OmenChest"))
                    {
                        this.EntitySubtype = EntitySubtypes.ImportantStrongbox;
                    }
                    else if (this.Path.StartsWith("Metadata/Chests/LeagueAzmeri/"))
                    {
                        this.EntitySubtype = EntitySubtypes.ChestWithLabel;
                    }
                    else if (this.Path.StartsWith("Metadata/Chests/LeaguesExpedition"))
                    {
                        this.EntitySubtype = EntitySubtypes.ExpeditionChest;
                    }
                    else if (this.TryGetComponent<MinimapIcon>(out var _))
                    {
                        return false;
                    }
                    else if (this.Path.StartsWith("Metadata/Chests/LegionChests"))
                    {
                        return false;
                    }
                    else if (this.Path.StartsWith("Metadata/Chests/DelveChests/"))
                    {
                        this.EntitySubtype = EntitySubtypes.DelveChest;
                    }
                    else if (this.Path.StartsWith("Metadata/Chests/Breach"))
                    {
                        this.EntitySubtype = EntitySubtypes.BreachChest;
                    }
                    else if (chestComp.IsStrongbox || this.Path.StartsWith("Metadata/Chests/SynthesisChests/SynthesisChestAmbush"))
                    {
                        if (this.Path.StartsWith("Metadata/Chests/StrongBoxes/Arcanist") ||
                            this.Path.StartsWith("Metadata/Chests/StrongBoxes/Cartographer") ||
                            this.Path.StartsWith("Metadata/Chests/StrongBoxes/StrongboxDivination") ||
                            this.Path.StartsWith("Metadata/Chests/StrongBoxes/StrongboxScarab"))
                        {
                            this.EntitySubtype = EntitySubtypes.ImportantStrongbox;
                        }
                        else
                        {
                            this.EntitySubtype = EntitySubtypes.Strongbox;
                        }
                    }
                    else if (chestComp.IsLabelVisible)
                    {
                        this.EntitySubtype = EntitySubtypes.ChestWithLabel;
                    }
                    else
                    {
                        this.EntitySubtype = EntitySubtypes.None;
                    }

                    break;
                case EntityTypes.Player:
                    if (this.Id == Core.States.InGameStateObject.CurrentAreaInstance.Player.Id)
                    {
                        this.EntitySubtype = EntitySubtypes.PlayerSelf;
                    }
                    else
                    {
                        this.EntitySubtype = EntitySubtypes.PlayerOther;
                    }

                    break;
                case EntityTypes.ImportantMiscellaneousObject:
                    // "Metadata/MiscellaneousObjects/Azmeri/AzmeriDustConverter" is already done.
                    if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriLightBomb"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriLightBomb;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriFuelResupply"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriRefuel;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriResource"))
                    {
                        if (this.TryGetComponent<Animated>(out var ani))
                        {
                            if (ani.Path.Contains("wisp_primal_sml"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriBlueWispSml;
                            }
                            else if (ani.Path.Contains("wisp_primal_med"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriBlueWispMed;
                            }
                            else if (ani.Path.Contains("wisp_primal_big"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriBlueWispBig;
                            }
                            else if (ani.Path.Contains("wisp_warden_sml"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriYellowWispSml;
                            }
                            else if (ani.Path.Contains("wisp_warden_med"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriYellowWispMed;
                            }
                            else if (ani.Path.Contains("wisp_warden_big"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriYellowWispBig;
                            }
                            else if (ani.Path.Contains("wisp_vodoo_sml"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriPurpleWispSml;
                            }
                            else if (ani.Path.Contains("wisp_vodoo_med"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriPurpleWispMed;
                            }
                            else if (ani.Path.Contains("wisp_vodoo_big"))
                            {
                                this.EntitySubtype = EntitySubtypes.AzmeriPurpleWispBig;
                            }
                            else
                            {
                                return false;
                            }
                        }
                        else
                        {
                            return false;
                        }
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Abyss/AbyssStartNode"))
                    {
                        this.EntitySubtype = EntitySubtypes.AbyssStartNode;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Abyss/AbyssFinalNode"))
                    {
                        this.EntitySubtype = EntitySubtypes.AbyssFinalNode;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Abyss/AbyssCrack") ||
                        this.Path.StartsWith("Metadata/MiscellaneousObjects/Abyss/AbyssNodeMini"))
                    {
                        this.EntitySubtype = EntitySubtypes.AbyssCrack;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Abyss/AbyssNode"))
                    {
                        this.EntitySubtype = EntitySubtypes.AbyssMidNode;
                    }
                    else
                    {
                        this.EntitySubtype = EntitySubtypes.None;
                    }

                    break;
                case EntityTypes.Shrine:
                    if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/SacrificeAltarObjects/AzmeriSacrificeAltar"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriSacrificeAltar;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriFlaskRefill"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriWell;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriBuffEffigySmall"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriExperienceGainShrine;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriBuffEffigyMedium"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriIncreaseQuanityShrine;
                    }
                    else if (this.Path.StartsWith("Metadata/MiscellaneousObjects/Azmeri/AzmeriBuffEffigyLarge"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriCanNotBeDamagedShrine;
                    }
                    else if (this.Path.StartsWith(AzmeriMiscellaneousObjectStarting))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriUnknownShrine;
                    }
                    else
                    {
                        this.EntitySubtype = EntitySubtypes.None;
                    }

                    break;
                case EntityTypes.Blockage:
                case EntityTypes.DeliriumSpawner:
                case EntityTypes.DeliriumBomb:
                case EntityTypes.Renderable:
                    break;
                case EntityTypes.Monster:
                    if (!this.TryGetComponent<ObjectMagicProperties>(out var omp))
                    {
                        // All monsters must have OMP, otherwise they are not monsters.
                        // This check happens at EntityType detection.
                        return false;
                    }

                    if (!this.TryGetComponent<Stats>(out var statcomp, false))
                    {
                        // All monsters must have stats component, otherwise they are not monsters.
                        return false;
                    }

                    if (this.Path.StartsWith(LegionLeagueMonsterStarting) &&
                        this.TryGetComponent<Buffs>(out var buffComp))
                    {
                        if (buffComp.StatusEffects.ContainsKey("legion_reward_display"))
                        {
                            this.EntitySubtype = EntitySubtypes.LegionChest;
                        }
                        else if (this.Path.Contains("ChestEpic"))
                        {
                            this.EntitySubtype = EntitySubtypes.LegionEpicChest;
                        }
                        else if (this.Path.Contains("Chest"))
                        {
                            this.EntitySubtype = EntitySubtypes.LegionChest;
                        }
                        else
                        {
                            this.EntitySubtype = EntitySubtypes.LegionMonster;
                        }
                    }
                    else if (omp.ModStats.TryGetValue(GameStats.is_bestiary_yellow_beast, out var yellow) && yellow == 1)
                    {
                        this.EntitySubtype = EntitySubtypes.YellowBestiaryMonster;
                    }
                    else if (statcomp.AllStats.TryGetValue(GameStats.is_capturable_monster, out var cap) && cap == 1)
                    {
                        this.EntitySubtype = EntitySubtypes.RedBestiaryMonster;
                    }
                    else if (this.Path.StartsWith(HarbingerLeagueMonsterStarting) &&
                        omp.ModNames.Contains("MonsterCannotBeDamaged"))
                    {
                        this.EntitySubtype = EntitySubtypes.HarbingerMonster;
                    }
                    else if (this.Path.StartsWith(TormentedSpirit))
                    {
                        this.EntitySubtype = EntitySubtypes.TormentedSpiritsMonster;
                    }
                    else if (this.Path.StartsWith(BetrayalLeagueMonsterStarting) &&
                        this.componentAddresses.ContainsKey("NPC"))
                    {
                        this.EntitySubtype = EntitySubtypes.BetrayalEnemyNPC;
                    }
                    else if (ArchnemesisUselessMonster.Any(this.Path.StartsWith))
                    {
                        return false;
                    }
                    else if (MavenUselessMonsters.Any(this.Path.StartsWith))
                    {
                        return false;
                    }
                    else if (this.Path.Contains(SirusUselessMonster))
                    {
                        return false;
                    }
                    else if (Tier17UselessMonsters.Any(this.Path.StartsWith) &&
                        !this.Path.Contains("Standalone")) // Fix to only remove the apparitions, not the actual elderslayers in valdo maps.
                    {
                        return false;
                    }
                    else if (CortexUselessMonsters.Any(this.Path.StartsWith))
                    {
                        return false;
                    }
                    else if (this.Path.Contains(RitualUselessMonster))
                    {
                        return false;
                    }
                    else if (omp.ModNames.Contains("PinnacleAtlasBoss"))
                    {
                        this.EntitySubtype = EntitySubtypes.PinnacleBoss;
                    }
                    else
                    {
                        this.EntitySubtype = EntitySubtypes.None;
                    }

                    for (var i = 0; i < Core.GHSettings.PoiMonstersCategories2.Count; i++)
                    {
                        var (filtertype, filter, rarity, stat, group) = Core.GHSettings.PoiMonstersCategories2[i];
                        if (filtertype switch
                        {
                            EntityFilterType.PATH => this.Path.StartsWith(filter),
                            EntityFilterType.PATHANDRARITY => omp.Rarity == rarity && this.Path.StartsWith(filter),
                            EntityFilterType.MOD => omp.ModNames.Contains(filter),
                            EntityFilterType.MODANDRARITY => omp.Rarity == rarity && omp.ModNames.Contains(filter),
                            EntityFilterType.PATHANDSTAT => (omp.ModStats.ContainsKey(stat) ||
                                                             statcomp.AllStats.ContainsKey(stat)) &&
                                                             this.Path.StartsWith(filter),
                            _ => throw new Exception($"EntityFilterType {filtertype} added but not handled in Entity file.")
                        })
                        {
                            this.oldSubtypeWithoutPOI = this.EntitySubtype;
                            this.EntitySubtype = EntitySubtypes.POIMonster;
                            this.customGroup = group;
                        }
                    }

                    break;
                case EntityTypes.NPC:
                    if (Core.GHSettings.SpecialNPCPaths.Any(this.Path.StartsWith))
                    {
                        this.EntitySubtype = EntitySubtypes.SpecialNPC;
                    }
                    else if (this.Path.StartsWith("Metadata/NPC/League/Azmeri/UniqueDealer"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriTraderNPC;
                    }
                    else if (this.Path.StartsWith("Metadata/NPC/League/Affliction/GlyphsHarvestTree"))
                    {
                        this.EntitySubtype = EntitySubtypes.AzmeriHarvestNPC;
                    }
                    else
                    {
                        this.EntitySubtype = EntitySubtypes.None;
                    }

                    break;
                case EntityTypes.Item:
                    this.EntitySubtype = EntitySubtypes.WorldItem;
                    break;
                default:
                    throw new Exception($"Please update TryCalculateEntitySubType function to include {this.EntityType}.");
            }

            return true;
        }

        private void CalculateEntityState()
        {
            if (this.EntityType == EntityTypes.Chest)
            {
                if (!this.TryGetComponent<Chest>(out var chestComp))
                {
                }
                else if (chestComp.IsOpened)
                {
                    this.EntityState = EntityStates.Useless;
                }
            }
            else if (this.EntityType == EntityTypes.DeliriumBomb || this.EntityType == EntityTypes.DeliriumSpawner)
            {
                if (!this.TryGetComponent<Life>(out var lifeComp))
                {
                }
                else if (!lifeComp.IsAlive)
                {
                    this.EntityState = EntityStates.Useless;
                }
            }
            else if (this.EntityType == EntityTypes.Monster)
            {
                if (!this.TryGetComponent<Life>(out var lifeComp))
                {
                }
                else if (!lifeComp.IsAlive)
                {
                    this.EntityState = EntityStates.Useless;
                }
                else if (!this.TryGetComponent<Positioned>(out var posComp))
                {
                }
                else if (posComp.IsFriendly)
                {
                    this.EntityState = EntityStates.MonsterFriendly;
                }
                else if (this.EntityState == EntityStates.MonsterFriendly)
                {
                    this.EntityState = EntityStates.None;
                }
                else if (this.IsBestiaryOrUsedToBe())
                {
                    if (this.TryGetComponent<Buffs>(out var buffsComp) &&
                        buffsComp.StatusEffects.ContainsKey("capture_monster_trapped"))
                    {
                        this.EntityState = EntityStates.Useless;
                    }
                }
                else if (this.IsBetrayalEnemyNPCOrUsedToBe())
                {
                    if (this.TryGetComponent<Buffs>(out var buffsComp) &&
                        buffsComp.StatusEffects.ContainsKey("betrayal_target_safety_aura"))
                    {
                        this.EntityState = EntityStates.Useless;
                    }
                }
                else if (this.IsPinnacleBossOrUsedToBe())
                {
                    if (this.TryGetComponent<Buffs>(out var buffsComp) &&
                        buffsComp.StatusEffects.ContainsKey("hidden_monster"))
                    {
                        this.EntityState = EntityStates.PinnacleBossHidden;
                    }
                    else
                    {
                        this.EntityState = EntityStates.None;
                    }
                }
                else if (this.IsLegionRelatedOrUsedToBe())
                {
                    // When Legion monolith is not clicked by the user (Stage 0),
                    //     Legion monsters (a.k.a FIT) has Frozen in time + Hidden buff.

                    // When Legion monolith is clicked (Stage 1),
                    //     FIT Not Killed by User: Just have frozen in time buff.
                    //     FIT Killed by user: Just have hidden buff.

                    // When Legion monolith is destroyed (Stage 2),
                    //     FIT are basically same as regular monster with no Frozen-in-time/hidden buff.

                    // NOTE: There are other hidden monsters in the game as well
                    // e.g. Delirium monsters (a.k.a DELI), underground crabs, hidden sea witches
                    if (this.TryGetComponent<Buffs>(out var buffComp))
                    {
                        var isFrozenInTime = buffComp.StatusEffects.ContainsKey("frozen_in_time");
                        var isHidden = buffComp.StatusEffects.ContainsKey("hidden_monster");
                        if (isFrozenInTime && isHidden)
                        {
                            this.EntityState = EntityStates.LegionStage0;
                        }
                        else if (isFrozenInTime)
                        {
                            this.EntityState = EntityStates.LegionStage1Alive;
                        }
                        else if (isHidden)
                        {
                            this.EntityState = EntityStates.LegionStage1Dead;
                        }
                        else
                        {
                            this.EntityState = EntityStates.None;
                        }
                    }
                }
            }
            else if (this.EntitySubtype == EntitySubtypes.PlayerOther)
            {
                if (!this.TryGetComponent<Player>(out var playerComp))
                {
                }
                else if (playerComp.Name.Equals(Core.GHSettings.LeaderName))
                {
                    this.EntityState = EntityStates.PlayerLeader;
                }
                else
                {
                    this.EntityState = EntityStates.None;
                }
            }
        }

        private bool IsBestiaryOrUsedToBe()
        {
            var toCheck = this.EntitySubtype == EntitySubtypes.POIMonster ?
                this.oldSubtypeWithoutPOI : this.EntitySubtype;
            return toCheck == EntitySubtypes.YellowBestiaryMonster ||
                toCheck == EntitySubtypes.RedBestiaryMonster;
        }

        private bool IsBetrayalEnemyNPCOrUsedToBe()
        {
            var toCheck = this.EntitySubtype == EntitySubtypes.POIMonster ?
                this.oldSubtypeWithoutPOI : this.EntitySubtype;
            return toCheck == EntitySubtypes.BetrayalEnemyNPC;
        }

        private bool IsPinnacleBossOrUsedToBe()
        {
            var toCheck = this.EntitySubtype == EntitySubtypes.POIMonster ?
                this.oldSubtypeWithoutPOI : this.EntitySubtype;
            return toCheck == EntitySubtypes.PinnacleBoss;
        }

        private bool IsLegionRelatedOrUsedToBe()
        {
            var toCheck = this.EntitySubtype == EntitySubtypes.POIMonster ? this.oldSubtypeWithoutPOI : this.EntitySubtype;
            return toCheck == EntitySubtypes.LegionChest ||
                    toCheck == EntitySubtypes.LegionEpicChest ||
                    toCheck == EntitySubtypes.LegionMonster;
        }
    }
}
