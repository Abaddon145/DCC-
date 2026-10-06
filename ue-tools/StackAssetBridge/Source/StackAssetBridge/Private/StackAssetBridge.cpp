#include "Modules/ModuleManager.h"
#include "AssetRegistry/AssetData.h"
#include "ContentBrowserModule.h"
#include "Framework/MultiBox/MultiBoxBuilder.h"
#include "Dom/JsonObject.h"
#include "HAL/FileManager.h"
#include "HAL/PlatformMisc.h"
#include "Misc/FileHelper.h"
#include "Misc/MessageDialog.h"
#include "Misc/Paths.h"
#include "Misc/EngineVersion.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

class FStackAssetBridgeModule : public IModuleInterface
{
    FDelegateHandle MenuHandle;
public:
    void StartupModule() override
    {
        auto& Browser = FModuleManager::LoadModuleChecked<FContentBrowserModule>("ContentBrowser");
        FContentBrowserMenuExtender_SelectedAssets Delegate = FContentBrowserMenuExtender_SelectedAssets::CreateRaw(this, &FStackAssetBridgeModule::Extend);
        MenuHandle = Delegate.GetHandle();
        Browser.GetAllAssetViewContextMenuExtenders().Add(Delegate);
    }
    void ShutdownModule() override
    {
        if (auto* Browser = FModuleManager::GetModulePtr<FContentBrowserModule>("ContentBrowser"))
        {
            Browser->GetAllAssetViewContextMenuExtenders().RemoveAll([this](const auto& Delegate) { return Delegate.GetHandle() == MenuHandle; });
        }
    }
    TSharedRef<FExtender> Extend(const TArray<FAssetData>& Assets)
    {
        TSharedRef<FExtender> Extender = MakeShared<FExtender>();
        if (!Assets.IsEmpty())
        {
            Extender->AddMenuExtension("GetAssetActions", EExtensionHook::After, nullptr, FMenuExtensionDelegate::CreateLambda([Assets](FMenuBuilder& Menu)
            {
                Menu.AddMenuEntry(FText::FromString(TEXT("收录到栈藏 UE 资产库")), FText::FromString(TEXT("发送选中资产的原生路径，不移动、修改或上传工程文件。")), FSlateIcon(), FUIAction(FExecuteAction::CreateLambda([Assets] { Collect(Assets); })));
            }));
        }
        return Extender;
    }
    static void Collect(const TArray<FAssetData>& Assets)
    {
        FString BindingText;
        TSharedPtr<FJsonObject> Binding;
        const FString BindingPath = FPaths::ProjectPluginsDir() / TEXT("StackAssetBridge/Resources/stack-library.json");
        if (!FFileHelper::LoadFileToString(BindingText, *BindingPath) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(BindingText), Binding) || !Binding.IsValid())
        {
            FMessageDialog::Open(EAppMsgType::Ok, FText::FromString(TEXT("请先在栈藏的 UE 来源中确认安装右键收录入口。未写入任何资产文件。")));
            return;
        }
        FString LibraryId, Token;
        FGuid Parsed;
        if (!Binding->TryGetStringField(TEXT("libraryId"), LibraryId) || !FGuid::Parse(LibraryId, Parsed) || !Binding->TryGetStringField(TEXT("token"), Token) || !FGuid::Parse(Token, Parsed) || Assets.Num() > 5000) return;
        TArray<TSharedPtr<FJsonValue>> Paths;
        for (const auto& Asset : Assets)
        {
            if (!Asset.PackageName.ToString().StartsWith(TEXT("/Game/")))
            {
                FMessageDialog::Open(EAppMsgType::Ok, FText::FromString(TEXT("首版收录仅处理当前工程 /Game 下的资产；引擎或插件资产请先用 UE 迁移到工程 Content。")));
                return;
            }
            Paths.Add(MakeShared<FJsonValueString>(Asset.GetObjectPathString()));
        }
        const FString Id = FGuid::NewGuid().ToString(EGuidFormats::DigitsWithHyphens).ToLower();
        TSharedRef<FJsonObject> Receipt = MakeShared<FJsonObject>();
        Receipt->SetStringField(TEXT("id"), Id);
        Receipt->SetStringField(TEXT("libraryId"), LibraryId);
        Receipt->SetStringField(TEXT("token"), Token);
        Receipt->SetStringField(TEXT("project"), FPaths::ConvertRelativePathToFull(FPaths::GetProjectFilePath()));
        Receipt->SetStringField(TEXT("engineVersion"), FString::Printf(TEXT("%d.%d"), FEngineVersion::Current().GetMajor(), FEngineVersion::Current().GetMinor()));
        Receipt->SetArrayField(TEXT("objectPaths"), Paths);
        FString Json;
        FJsonSerializer::Serialize(Receipt, TJsonWriterFactory<>::Create(&Json));
        const FString Directory = FPlatformMisc::GetEnvironmentVariable(TEXT("LOCALAPPDATA")) / TEXT("DCCAssetLibrary/ue-capture") / LibraryId;
        IFileManager::Get().MakeDirectory(*Directory, true);
        const FString Output = Directory / (Id + TEXT(".json"));
        const FString Temporary = Output + TEXT(".partial");
        if (FFileHelper::SaveStringToFile(Json, *Temporary, FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM) && IFileManager::Get().Move(*Output, *Temporary))
        {
            FMessageDialog::Open(EAppMsgType::Ok, FText::FromString(TEXT("已发送至栈藏待收录列表。在 UE 资产库点击确认收录即可；来源文件没有改变。")));
        }
        else FMessageDialog::Open(EAppMsgType::Ok, FText::FromString(TEXT("无法保存收录请求，请检查本机设置目录的空间或权限。")));
    }
};
IMPLEMENT_MODULE(FStackAssetBridgeModule, StackAssetBridge)
