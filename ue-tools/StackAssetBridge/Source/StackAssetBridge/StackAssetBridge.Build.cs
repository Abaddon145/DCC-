using UnrealBuildTool;

public class StackAssetBridge : ModuleRules
{
    public StackAssetBridge(ReadOnlyTargetRules Target) : base(Target)
    {
        PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
        PublicDependencyModuleNames.AddRange(new[] { "Core", "CoreUObject", "Engine" });
        PrivateDependencyModuleNames.AddRange(new[] { "UnrealEd", "ImageCore", "ImageWrapper", "RenderCore", "RHI", "ContentBrowser", "Slate", "SlateCore", "Json" });
    }
}
