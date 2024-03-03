#include <assert.h>
#include <stdint.h>
#define _USE_MATH_DEFINES
#include <math.h>

#include "stb_image.h"

#if defined(DEBUG) || defined(_DEBUG)
#define RENDERDOC_DEBUG 0
#include "renderdoc.h"
RENDERDOC_API_1_1_2* rdoc_api = NULL;
#else
#define RENDERDOC_DEBUG 0
#endif

extern "C" {
#include "tetramesh.h"
}

#include <set>
#include <map>
#include <string>
#include <stack>
#include <utility>

#include "mc_renderer.h"

#include <Windows.h>

struct vert_uniform {
    /* array of columns */
    DirectX::XMMATRIX modelView;
    DirectX::XMMATRIX proj;
    DirectX::XMMATRIX norm;
	float2 viewport_size;
	float2 inlet_size;
	float stroke_radius;
	float max_miter_scale;
	float dot_radius;
    float z_offset;
    /* make size a multiple of 16 */
    float _pad[2];
};

struct frag_uniform {
	float opacity, gloss;
    /* make size 16 */
    float _pad[2];
};

struct frame_in {
	float2 pos;
	float4 col;
};

struct dot_in {
    float4 col;
    float3 pos;
    float3 norm;
    float theta;
};

struct lin_in {
    float4 start_col;
	float4 end_col;

	float3 start;
	float3 end;

	float3 next_tan;
	float3 tangent;
	float3 prev_tan;
};

struct tri_in {
    float4 col;
    float3 pos;
    float3 norm;
    float2 uv;
};

constexpr int min_padding = 10;
constexpr int min_padding_presentation = 45;
constexpr int max_buffer = 1 << 16;
constexpr int max_texture = 1 << 8;

std::stack<ResourceManager*> g_rm;
class ResourceManager {
    ID3D11Device* d;
    ID3D11DeviceContext* c;
    std::set<uint32_t> available_ids;
    std::map<uint32_t, buffer> active;

    std::set<uint32_t> available_textures;
    std::map<uint32_t, std::pair<ID3D11ShaderResourceView*, ID3D11Texture2D*>> textures;
    std::map<std::string, uint32_t> texture_handles;

    CRITICAL_SECTION lock;

public:
    ResourceManager(ID3D11Device* d, ID3D11DeviceContext* c) : d{ d }, c{ c } {
        for (uint32_t i = 2; i < max_buffer; ++i) {
            available_ids.insert(i);
        }

        for (uint32_t i = 0; i < max_texture; ++i) {
            available_textures.insert(i);
        }

        this->create_texture("res/blank1x1.png", 0);
        this->create_texture("res/image_not_found.png", 1);

		InitializeCriticalSection(&lock);
    
        g_rm.push(this);
    }

    uint32_t register_id() {
        EnterCriticalSection(&lock);

        if (available_ids.size()) {
            uint32_t m = *available_ids.begin();
            available_ids.erase(available_ids.begin());
            active[m] = {};

            LeaveCriticalSection(&lock);
            return m;
        }

		LeaveCriticalSection(&lock);
        return 0;
    }

    std::pair<ID3D11ShaderResourceView*, ID3D11Texture2D*> get_texture_for_handle(mc_handle_t handle) {
        return textures[handle];
    }

    mc_handle_t create_texture(std::string const& path, mc_handle_t wanted_handle) {
		int w, h, c;
		unsigned char* bytes = stbi_load(path.c_str(), &w, &h, &c, 4);
		if (bytes) {
            available_textures.erase(wanted_handle);

            D3D11_TEXTURE2D_DESC textureDesc = {};
            textureDesc.Width = w;
            textureDesc.Height = h;
            textureDesc.MipLevels = 1;
            textureDesc.ArraySize = 1;
            textureDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;
            textureDesc.SampleDesc.Count = 1;
            textureDesc.Usage = D3D11_USAGE_IMMUTABLE;
            textureDesc.BindFlags = D3D11_BIND_SHADER_RESOURCE;

            D3D11_SUBRESOURCE_DATA textureSubresourceData = {};
            textureSubresourceData.pSysMem = bytes;
            textureSubresourceData.SysMemPitch = c * w;

            ID3D11Texture2D* texture;
            d->CreateTexture2D(&textureDesc, &textureSubresourceData, &texture);

            ID3D11ShaderResourceView* textureView;
            d->CreateShaderResourceView(texture, nullptr, &textureView);

            textures[wanted_handle] = std::make_pair(textureView, texture);

            return wanted_handle;
        }
        else {
            /* image not found */
            return 1;
        }
    }

    mc_handle_t poll_texture(std::string const &path) {
        if (texture_handles.find(path) != texture_handles.end()) {
            return texture_handles[path];
        }
        else if (available_textures.empty()) {
            return 0;
        }
        else {
            return create_texture(path, *available_ids.begin());
        }
    }

    void deregister(uint32_t id) {
        EnterCriticalSection(&lock);

        if (active[id].b) active[id].b->Release();
        active.erase(id);
        available_ids.insert(id);

		LeaveCriticalSection(&lock);
    }

	template<typename T>
    void write(T* bytes, int numel, uint32_t id, mc_bool_t constant) {
        EnterCriticalSection(&lock);

        buffer& ref = active[id];
        if (constant) {
            if (ref.b) ref.b->Release();
            assert(numel == 1);
            ref = buffer::cbuffer(d, bytes[0]);
        }
        else {
			if (numel * sizeof(T) > ref.elems * ref.size) {
				if (ref.b) ref.b->Release();
				ref = buffer::vertex_buffer(d, numel, bytes);
			}
			else {
				ref.write_fixed(c, numel, bytes);
			}
        }

		LeaveCriticalSection(&lock);
    }

    buffer buffer_for(uint32_t id) {
        EnterCriticalSection(&lock);

        buffer ret = active[id];

		LeaveCriticalSection(&lock);
        return ret;
    }

    ~ResourceManager() {
        /* exporting is a bit weird */
        g_rm.pop();

        for (auto& b : active) {
            if (b.second.b) b.second.b->Release();
        }

        for (auto& t : textures) {
            t.second.first->Release();
            t.second.second->Release();
        }

        DeleteCriticalSection(&lock);
    }
};

static int
compile_vertex_shader(ID3D11Device* device, LPCWSTR file, char const* name, ID3D11VertexShader** vs, ID3DBlob** blob)
{
	ID3DBlob* shaderCompileErrorsBlob;
	HRESULT hResult = D3DCompileFromFile(file, nullptr, nullptr, name, "vs_5_0", 0, 0, blob, &shaderCompileErrorsBlob);
	if (FAILED(hResult))
	{
		const char* errorString = NULL;
		if (hResult == HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND))
			errorString = "Could not compile shader; file not found";
		else if (shaderCompileErrorsBlob) {
			errorString = (const char*)shaderCompileErrorsBlob->GetBufferPointer();
			shaderCompileErrorsBlob->Release();
		}
		MessageBoxA(0, errorString, "Shader Compiler Error", MB_ICONERROR | MB_OK);
		return 1;
	}

	hResult = device->CreateVertexShader((*blob)->GetBufferPointer(), (*blob)->GetBufferSize(), nullptr, vs);
	assert(SUCCEEDED(hResult));
    return 0;
}

static int
compile_frag_shader(ID3D11Device* device, LPCWSTR file, char const* name, ID3D11PixelShader** ps)
{
    ID3DBlob* psBlob;
    ID3DBlob* shaderCompileErrorsBlob;
    HRESULT hResult = D3DCompileFromFile(file, nullptr, nullptr, name, "ps_5_0", 0, 0, &psBlob, &shaderCompileErrorsBlob);
    if (FAILED(hResult))
    {
        const char* errorString = NULL;
        if (hResult == HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND))
            errorString = "Could not compile shader; file not found";
        else if (shaderCompileErrorsBlob) {
            errorString = (const char*)shaderCompileErrorsBlob->GetBufferPointer();
            shaderCompileErrorsBlob->Release();
        }
        MessageBoxA(0, errorString, "Shader Compiler Error", MB_ICONERROR | MB_OK);
        return 1;
    }

    hResult = device->CreatePixelShader(psBlob->GetBufferPointer(), psBlob->GetBufferSize(), nullptr, ps);
    assert(SUCCEEDED(hResult));
    psBlob->Release();
   
    return 0;
}

MCRenderer::MCRenderer() {
    this->init_device();
    this->init_shader();

    this->resource_manager = new ResourceManager(d, c);

#if RENDERDOC_DEBUG
    if (!rdoc_api) {
        if (HMODULE mod = GetModuleHandleA("renderdoc.dll"))
        {
            pRENDERDOC_GetAPI RENDERDOC_GetAPI =
                (pRENDERDOC_GetAPI)GetProcAddress(mod, "RENDERDOC_GetAPI");
            int ret = RENDERDOC_GetAPI(eRENDERDOC_API_Version_1_1_2, (void**)&rdoc_api);
            assert(ret == 1);
        }
    }
#endif
}

void MCRenderer::init_device() {
	D3D_FEATURE_LEVEL featureLevels[] = { D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0 };
    UINT deviceFlags = 0;

#if (defined(DEBUG) || defined(_DEBUG))
    deviceFlags |= D3D11_CREATE_DEVICE_DEBUG;
#endif      

    HRESULT hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, deviceFlags, featureLevels, 3, D3D11_SDK_VERSION, &d, nullptr, &c);
    assert(SUCCEEDED(hr));

    {
		c->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    }

#if defined(DEBUG) || defined(_DEBUG)
    d->QueryInterface(__uuidof(ID3D11Debug), (void**)&debug);
    if (debug)
    {
        ID3D11InfoQueue* d3dInfoQueue = nullptr;
        if (SUCCEEDED(debug->QueryInterface(__uuidof(ID3D11InfoQueue), (void**)&d3dInfoQueue)))
        {
            d3dInfoQueue->SetBreakOnSeverity(D3D11_MESSAGE_SEVERITY_CORRUPTION, true);
            d3dInfoQueue->SetBreakOnSeverity(D3D11_MESSAGE_SEVERITY_ERROR, true);
            d3dInfoQueue->Release();
        }
        debug->Release();
    }
#endif

    {
        ID3D11BlendState* bstate = NULL;

        D3D11_BLEND_DESC blendStateDesc = {};
        blendStateDesc.AlphaToCoverageEnable = FALSE;
        blendStateDesc.IndependentBlendEnable = FALSE;
        blendStateDesc.RenderTarget[0].BlendEnable = TRUE;
        blendStateDesc.RenderTarget[0].SrcBlend = D3D11_BLEND_SRC_ALPHA;
        blendStateDesc.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
        blendStateDesc.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
        blendStateDesc.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_SRC_ALPHA;
        blendStateDesc.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_DEST_ALPHA;
        blendStateDesc.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
        blendStateDesc.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;

        HRESULT hr = d->CreateBlendState(&blendStateDesc, &bstate);
        assert(SUCCEEDED(hr));
        c->OMSetBlendState(bstate, NULL, 0xFFFFFFFF);
        bstate->Release();
    }

    {
        D3D11_RASTERIZER_DESC rasterizerDescription = {};
        rasterizerDescription.MultisampleEnable = true;
        rasterizerDescription.AntialiasedLineEnable = true;
        rasterizerDescription.FrontCounterClockwise = true;
        rasterizerDescription.CullMode = D3D11_CULL_BACK;
        rasterizerDescription.FillMode = D3D11_FILL_SOLID;
        ID3D11RasterizerState* rs;
        HRESULT hr = d->CreateRasterizerState(&rasterizerDescription, &rs);
        assert(SUCCEEDED(hr));
        c->RSSetState(rs);
        rs->Release();
    }

    {
        D3D11_SAMPLER_DESC samplerDesc = {};
        samplerDesc.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
        samplerDesc.AddressU = D3D11_TEXTURE_ADDRESS_BORDER;
        samplerDesc.AddressV = D3D11_TEXTURE_ADDRESS_BORDER;
        samplerDesc.AddressW = D3D11_TEXTURE_ADDRESS_BORDER;
        samplerDesc.BorderColor[0] = 1.0f;
        samplerDesc.BorderColor[1] = 1.0f;
        samplerDesc.BorderColor[2] = 1.0f;
        samplerDesc.BorderColor[3] = 1.0f;
        samplerDesc.ComparisonFunc = D3D11_COMPARISON_NEVER;

        d->CreateSamplerState(&samplerDesc, &sampler);
    }

    {
        /* for windows, just not letting them set the dot vertex count */
        float thetas[8];
        constexpr int count = sizeof(thetas) / sizeof(thetas[0]);
        for (int i = 0; i < count; ++i) {
            thetas[i] = (float)i * M_PI * 2 / count;
        }

        this->dot_verts = buffer::vertex_buffer(d, count, thetas);
        uint16_t indices[(count - 2) * 3];
        for (int i = 0; i < count - 2; ++i) {
            indices[3 * i] = 0;
            indices[3 * i + 1] = i + 1;
            indices[3 * i + 2] = (i + 2) % count;
        }
        this->dot_indices = buffer::index_buffer(d, (count - 2) * 3, indices);
    }

    {
        int32_t lins[6];
        constexpr int count = sizeof(lins) / sizeof(lins[0]);
        for (int i = 0; i < count; ++i) {
            lins[i] = i;
        }

        this->lin_verts = buffer::vertex_buffer(d, count, lins);

        uint16_t indices[12] = {
            0, 1, 2,
            0, 2, 3,
            3, 2, 4,
            3, 4, 5
        };
        this->lin_indices = buffer::index_buffer(d, 12, indices);
    }
}

void MCRenderer::init_shader(void) {
    ID3DBlob* frameBlob;
    compile_vertex_shader(d, L"res/frame_shader.hlsl", "frame_vert", &frame_vert_shader, &frameBlob);
    !compile_frag_shader(d, L"res/frame_shader.hlsl", "frame_frag", &frame_frag_shader);

    {
        D3D11_INPUT_ELEMENT_DESC inputElementDesc[] =
        {
            { "POS", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0 },
            { "COL", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_VERTEX_DATA, 0 }
        };

        HRESULT hr = d->CreateInputLayout(inputElementDesc, ARRAYSIZE(inputElementDesc), frameBlob->GetBufferPointer(), frameBlob->GetBufferSize(), &frame_in_layout);
        assert(SUCCEEDED(hr));
        frameBlob->Release();
    }

    ID3DBlob* triBlob;
    compile_vertex_shader(d, L"res/tri_shader.hlsl", "tri_vert", &tri_vert_shader, &triBlob);
    compile_frag_shader(d, L"res/tri_shader.hlsl", "tri_frag", &tri_frag_shader);
    {
        D3D11_INPUT_ELEMENT_DESC inputElementDesc[] =
        {
            { "COL", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0 },
            { "POS", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_VERTEX_DATA, 0 },
            { "NORMAL", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_VERTEX_DATA, 0 },
            { "TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_VERTEX_DATA, 0 },
        };

        HRESULT hr = d->CreateInputLayout(inputElementDesc, ARRAYSIZE(inputElementDesc), triBlob->GetBufferPointer(), triBlob->GetBufferSize(), &tri_in_layout);
        assert(SUCCEEDED(hr));
        triBlob->Release();
    }

    ID3DBlob* linBlob;
    !compile_vertex_shader(d, L"res/lin_shader.hlsl", "lin_vert", &lin_vert_shader, &linBlob);
    !compile_frag_shader(d,  L"res/lin_shader.hlsl", "lin_frag", &lin_frag_shader);
    { 
        D3D11_INPUT_ELEMENT_DESC inputElementDesc[] =
        {
            { "COL", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, 0, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "COL", 1, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "POSITION", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "POSITION", 1, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "TANGENT", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "TANGENT", 1, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "TANGENT", 2, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "POSITION", 2, DXGI_FORMAT_R32_SINT, 1, 0, D3D11_INPUT_PER_VERTEX_DATA, 0 },
        };

        HRESULT hr = d->CreateInputLayout(inputElementDesc, ARRAYSIZE(inputElementDesc), linBlob->GetBufferPointer(), linBlob->GetBufferSize(), &lin_in_layout);
        assert(SUCCEEDED(hr));
        linBlob->Release();
    }

    ID3DBlob* dotBlob;
    compile_vertex_shader(d, L"res/dot_shader.hlsl", "dot_vert", &dot_vert_shader, &dotBlob);
    compile_frag_shader(d,  L"res/dot_shader.hlsl", "dot_frag", &dot_frag_shader);
    {
        D3D11_INPUT_ELEMENT_DESC inputElementDesc[] =
        {
            { "COL", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, 0, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "POSITION", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "NORMAL", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, 1 },
            { "POSITION", 1, DXGI_FORMAT_R32_FLOAT, 1, 0, D3D11_INPUT_PER_VERTEX_DATA, 0 },
        };

        HRESULT hr = d->CreateInputLayout(inputElementDesc, ARRAYSIZE(inputElementDesc), dotBlob->GetBufferPointer(), dotBlob->GetBufferSize(), &dot_in_layout);
        assert(SUCCEEDED(hr));
        dotBlob->Release();
    }
}

void MCRenderer::set_screen_size(int w, int h, bool matchInlet) {
    this->w = w;
    this->h = h;
    this->matching_inlet = matchInlet;

    c->OMSetRenderTargets(0, nullptr, nullptr);

    if (renderTargetView) renderTargetView->Release();
    if (depthStencilView) depthStencilView->Release();

    if (cpuBuffer) cpuBuffer->Release();
    if (resolveBuffer) resolveBuffer->Release();
    if (renderBuffer) renderBuffer->Release();

	UINT sc;
	for (sc = 8; sc; sc /= 2) {
		UINT max_quality = 0;

		HRESULT hr = d->CheckMultisampleQualityLevels(DXGI_FORMAT_B8G8R8A8_UNORM, sc, &max_quality);
		if (SUCCEEDED(hr) && max_quality != 0) {
			break;
		}
	}
	if (!sc) {
		sc = 1;
	}

    D3D11_TEXTURE2D_DESC rd = { 0 };
    HRESULT hr;
    {
        rd.Width = w;
        rd.Height = h;
        rd.MipLevels = 1;
        rd.ArraySize = 1;
        rd.SampleDesc.Count = sc;
        rd.SampleDesc.Quality = 0;
        rd.Usage = D3D11_USAGE_DEFAULT;
        rd.BindFlags = D3D11_BIND_RENDER_TARGET;
        rd.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        hr = d->CreateTexture2D(&rd, nullptr, &renderBuffer);
        assert(SUCCEEDED(hr));
    }

    {
        rd = { 0 };
        rd.Width = w;
        rd.Height = h;
        rd.MipLevels = 1;
        rd.ArraySize = 1;
        rd.SampleDesc.Count = 1;
        rd.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
        rd.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        rd.Usage = D3D11_USAGE_DEFAULT;
        rd.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        hr = d->CreateTexture2D(&rd, nullptr, &resolveBuffer);
        assert(SUCCEEDED(hr));
    }

    {
        rd = { 0 };
        rd.Width = w;
        rd.Height = h;
        rd.MipLevels = 1;
        rd.ArraySize = 1;
        rd.SampleDesc.Count = 1;
        rd.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        rd.Usage = D3D11_USAGE_STAGING;
        rd.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        hr = d->CreateTexture2D(&rd, nullptr, &cpuBuffer);
    }

    {
        D3D11_RENDER_TARGET_VIEW_DESC rtvDesc = { };
        rtvDesc.Format = rd.Format;
        rtvDesc.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2DMS;
        d->CreateRenderTargetView(renderBuffer, &rtvDesc, &renderTargetView);
    }
    
    {
        D3D11_TEXTURE2D_DESC depthTextureDesc = {};
        depthTextureDesc.Width = w;
        depthTextureDesc.Height = h;
        depthTextureDesc.MipLevels = 1;
        depthTextureDesc.ArraySize = 1;
        depthTextureDesc.SampleDesc.Count = sc;
        depthTextureDesc.SampleDesc.Quality = 0;
        depthTextureDesc.Format = DXGI_FORMAT_D32_FLOAT;
        depthTextureDesc.BindFlags = D3D11_BIND_DEPTH_STENCIL;
        depthTextureDesc.Usage = D3D11_USAGE_DEFAULT;

        ID3D11Texture2D* DepthStencilTexture;
        HRESULT hr = d->CreateTexture2D(&depthTextureDesc, nullptr, &DepthStencilTexture);
        assert(SUCCEEDED(hr));

        D3D11_DEPTH_STENCIL_DESC dsDesc = {};
        dsDesc.DepthEnable = true;
        dsDesc.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ALL;
        dsDesc.DepthFunc = D3D11_COMPARISON_LESS;

        ID3D11DepthStencilState* dstate;
        hr = d->CreateDepthStencilState(&dsDesc, &dstate);
        assert(SUCCEEDED(hr));
        c->OMSetDepthStencilState(dstate, 0);

        D3D11_DEPTH_STENCIL_VIEW_DESC dsvDesc = {};
        dsvDesc.Format = depthTextureDesc.Format;
        dsvDesc.ViewDimension = D3D11_DSV_DIMENSION_TEXTURE2DMS;
        dsvDesc.Texture2D.MipSlice = 0;

        hr = d->CreateDepthStencilView(DepthStencilTexture, &dsvDesc, &depthStencilView);
        assert(SUCCEEDED(hr));
        DepthStencilTexture->Release();
        dstate->Release();
    }

    c->OMSetRenderTargets(1, &renderTargetView, depthStencilView);

    {
        D3D11_VIEWPORT vp;
        vp.Width = (float) w;
        vp.Height = (float) h;
        vp.MinDepth = 0.0f;
        vp.MaxDepth = 1.0f;
        vp.TopLeftX = 0;
        vp.TopLeftY = 0;
        c->RSSetViewports(1, &vp);
    }

    this->update_frame();
    this->update_matrices();

}

void MCRenderer::set_presentation_mode(bool presenting)
{
    this->in_presentation_mode = presenting;
    this->update_frame();
}

void MCRenderer::update_matrices(void) {
    using namespace DirectX;
    
    XMVECTOR const z = XMVector3Normalize(XMVectorSet(forward.x, forward.y, forward.z, 0));
    XMVECTOR const x = XMVector3Cross(z, XMVectorSet(up.x, up.y, up.z, 0));
    XMVECTOR const y = XMVector3Cross(x, z);

	/* orthonormal so inverse is transpose */
    XMMATRIX full_rotation{
        XMVectorSetW(x, 0),
        XMVectorSetW(y, 0),
        XMVectorNegate(XMVectorSetW(z, 0)),
        XMVectorSet(0, 0, 0, 1)
    };
    XMMATRIX trans{
        XMVectorSet(1, 0, 0, -origin.x),
        XMVectorSet(0, 1, 0, -origin.y),
        XMVectorSet(0, 0, 1, -origin.z),
        XMVectorSet(0, 0, 0, 1),
    };

    mv = XMMatrixMultiply(full_rotation, trans);

	float f = far_;
	float n = near_;
    XMMATRIX standard{
        XMVectorSet(1, 0, 0, 0),
        XMVectorSet(0, aspect_ratio, 0, 0),
        XMVectorSet(0, 0, -f / (f - n), -f * n / (f - n)),
        XMVectorSet(0, 0, -1, 0)
    };

	//(1,-1) -> (internal rect coordinates)
    XMMATRIX viewport{
        XMVectorSet((float)vw / w, 0, 0, 0),
        XMVectorSet(0, (float)vh / h, 0, 0),
        XMVectorSet(0, 0, 1, 0),
        XMVectorSet(0, 0, 0, 1),
    };

    p = XMMatrixMultiply(viewport, standard); 

    /* inverse of transpose */
    norm = XMMatrixTranspose(XMMatrixInverse(nullptr, XMMatrixTranspose(mv)));
    mv = XMMatrixTranspose(mv);
    p = XMMatrixTranspose(p);
}

void MCRenderer::update_frame(void) {
	/* adjust frame */

    float4 col;
    if (in_presentation_mode || matching_inlet) {
        col = { 0, 0, 0, 1 };
    }
    else {
		switch (this->state) {
		case viewport::VIEWPORT_IDLE:
			col = { 0.8f, 0.8f, 0.8f, 0.6f };
			break;
		case viewport::VIEWPORT_COMPILER_ERROR:
			col = { 0.8f, 0.7f, 0.7f, 0.6f };
			break;
		case viewport::VIEWPORT_RUNTIME_ERROR:
			col = { 0.8f, 0.6f, 0.6f, 0.6f };
			break;
		case viewport::VIEWPORT_LOADING:
			col = { 0.4f, 0.4f, 0.8f, 0.6f };
			break;
		case viewport::VIEWPORT_PLAYING:
			col = { 0.9f, 0.9f, 0.9f, 0.6f };
            break;
		}
    }

    {
        float u, v;
        float padding = this->in_presentation_mode ? min_padding_presentation : min_padding;
        if ((float)w / h > this->aspect_ratio) {
            v = (1 - (float)padding / (h / 2.0f));
            u = v * aspect_ratio * h / w;
        }
        else {
            u = (1 - (float)padding / (w / 2.0f));
            v = u / aspect_ratio * w / h;
        }

        if (matching_inlet) {
            vw = w;
            vh = h;
        }
        else {
            this->vw = (int)(u * w);
            this->vh = (int)(v * h);
        }

        frame_in vertexData[] = {
            {{-1, -1,}, col}, {{1, -1,}, col}, {{1, -v,}, col},
            {{ 1, -v,}, col}, {{-1, -v}, col}, {{-1, -1}, col},

            {{-1, v,}, col}, {{1, v,}, col}, {{ 1, 1,}, col},
            {{ 1, 1,}, col}, {{-1, 1}, col}, {{-1, v}, col},

            {{-1, v,}, col}, {{ -1, -v}, col}, {{-u, -v}, col },
            {{-1, v,}, col}, {{ -u, -v}, col}, {{-u,  v}, col },

            {{u, v,}, col}, {{ u, -v}, col}, {{1, -v}, col },
            {{u, v,}, col}, {{ 1, -v}, col}, {{1,  v}, col },
        };

        if (this->frame_verts.b) {
            this->frame_verts.write_fixed(c, sizeof(vertexData) / sizeof(vertexData[0]), vertexData);
        }
        else {
            this->frame_verts = buffer::vertex_buffer(d, sizeof(vertexData) / sizeof(vertexData[0]), vertexData);
        }
    }
}

void MCRenderer::render_frame(void) { 
    if (matching_inlet) {
        return;
    }

    c->IASetInputLayout(frame_in_layout);

    c->VSSetShader(frame_vert_shader, nullptr, 0);
    c->PSSetShader(frame_frag_shader, nullptr, 0);

    UINT offset = 0;
    c->IASetVertexBuffers(0, 1, &frame_verts.b, &frame_verts.size, &offset);

    c->Draw(frame_verts.elems, 0);
}

void MCRenderer::recache(struct viewport *viewport) {
    this->bg.r = viewport->background_color.x;
    this->bg.g = viewport->background_color.y;
    this->bg.b = viewport->background_color.z;
    this->bg.a = viewport->background_color.w;

    this->aspect_ratio = (float) viewport->aspect_ratio;

    vec3 const u = viewport->camera.up;
	vec3 const o = viewport->camera.origin;
    vec3 const f = viewport->camera.forward;
    this->up = { u.x, u.y, u.z };
    this->origin = { o.x, o.y, o.z };
    this->forward = { f.x, f.y, f.z };
    this->far_ = viewport->camera.z_far;
    this->near_ = viewport->camera.z_near;

    this->camera = viewport->camera;
    this->state = viewport->state;
    this->viewport = viewport;

    this->update_frame();
    this->update_matrices();
}

tri_in const* 
tri_buffer_pointer_for(struct tetramesh const* mesh, size_t* count) {
    assert(mesh->tris);

    tri_in* const ret = (tri_in *) malloc(3 * sizeof(tri_in) * mesh->tri_count);
    tri_in* current = ret;

    typedef tetramesh::tetra_tri tetra_tri;
    typedef tetra_tri::tetra_tri_vertex tetra_tri_vertex;

    for (size_t i = 0; i < mesh->tri_count; ++i) {
        tetra_tri const tri = mesh->tris[i];

        tetra_tri_vertex const vert_a = tri.a;
        tetra_tri_vertex const vert_b = tri.b;
        tetra_tri_vertex const vert_c = tri.c;

        if (vert_a.col.w < FLT_EPSILON && vert_b.col.w < FLT_EPSILON && vert_c.col.w < FLT_EPSILON) continue;;

        tri_in const a = {
             float4{vert_a.col.x, vert_a.col.y, vert_a.col.z, vert_a.col.w},
             float3{vert_a.pos.x, vert_a.pos.y, vert_a.pos.z},
             float3{vert_a.norm.x, vert_a.norm.y, vert_a.norm.z},
             float2{vert_a.uv.x, vert_a.uv.y},
        };

        tri_in const b = {
             float4{vert_b.col.x, vert_b.col.y, vert_b.col.z, vert_b.col.w},
             float3{vert_b.pos.x, vert_b.pos.y, vert_b.pos.z},
             float3{vert_b.norm.x, vert_b.norm.y, vert_b.norm.z},
             float2{vert_b.uv.x, vert_b.uv.y},
        };

        tri_in const c = {
             float4{vert_c.col.x, vert_c.col.y, vert_c.col.z, vert_c.col.w},
             float3{vert_c.pos.x, vert_c.pos.y, vert_c.pos.z },
             float3{vert_c.norm.x, vert_c.norm.y, vert_c.norm.z},
             float2{vert_c.uv.x, vert_c.uv.y },
        };

        *current++ = a;
        *current++ = b;
        *current++ = c;

        ++*count;
    }

    return ret;
}

lin_in const* 
lin_buffer_pointer_for(struct tetramesh const* mesh, size_t* count) {
    assert(mesh->lins);

    lin_in* const ret = (lin_in *) malloc(sizeof(lin_in) * mesh->lin_count);
    lin_in* current = ret;

    typedef tetramesh::tetra_lin tetra_lin;
    typedef tetra_lin::tetra_lin_vertex tetra_lin_vertex;

    for (size_t i = 0; i < mesh->lin_count; ++i) {
        tetra_lin const line = mesh->lins[i];

        if (!line.is_dominant_sibling) continue;
        else if (line.a.col.w < FLT_EPSILON && line.b.col.w < FLT_EPSILON) continue;

        tetra_lin const prev = line.prev >= 0 ? mesh->lins[line.prev] : line;
        tetra_lin const next = line.next >= 0 ? mesh->lins[line.next] : line;

        tetra_lin_vertex const vert_p = prev.a;
        tetra_lin_vertex const vert_a = line.a;
        tetra_lin_vertex const vert_b = line.b;
        tetra_lin_vertex const vert_n = next.b;

        float3 const pos_p{ vert_p.pos.x, vert_p.pos.y, vert_p.pos.z };
        float3 const pos_a{ vert_a.pos.x, vert_a.pos.y, vert_a.pos.z };
        float3 const pos_b{ vert_b.pos.x, vert_b.pos.y, vert_b.pos.z };
        float3 const pos_n{ vert_n.pos.x, vert_n.pos.y, vert_n.pos.z };

        float3 const tan{ pos_b.x - pos_a.x, pos_b.y - pos_a.y, pos_b.z - pos_a.z };
        float3 const prev_tan{ pos_a.x - pos_p.x, pos_a.y - pos_p.y, pos_a.z - pos_p.z };
        float3 const next_tan{ pos_n.x - pos_b.x, pos_n.y - pos_b.y, pos_n.z - pos_b.z };

        float4 const col_a{ vert_a.col.x, vert_a.col.y, vert_a.col.z, vert_a.col.w };
        float4 const col_b{ vert_b.col.x, vert_b.col.y, vert_b.col.z, vert_b.col.w };

        struct lin_in const build = {
            col_a,
            col_b,
            pos_a, 
            pos_b, 
            next_tan,
            tan,
            prev_tan,
        };
        *current++ = build;

        ++*count;
    }

    return ret;
}

static dot_in const* 
dot_buffer_pointer_for(struct tetramesh const* mesh, size_t* count) {
    assert(mesh->dots);

    struct dot_in* const ret = (dot_in *) malloc(sizeof(dot_in) * mesh->dot_count);
    struct dot_in* current = ret;

    typedef tetramesh::tetra_dot tetra_dot;

    for (size_t i = 0; i < mesh->dot_count; ++i) {
        tetra_dot const dot = mesh->dots[i];

        if (dot.col.w < FLT_EPSILON) continue;

        dot_in const a = {
            float4{dot.col.x, dot.col.y, dot.col.z, dot.col.w},
            float3{dot.pos.x, dot.pos.y, dot.pos.z},
            float3{dot.norm.x, dot.norm.y, dot.norm.z}
        };

        *current++ = a;
        ++*count;
    }

    return ret;
}

void MCRenderer::set_uniforms(struct tetramesh* mesh)
{
	vert_uniform vu{};
	vu.modelView = this->mv;
	vu.proj = this->p;
	vu.norm = this->norm;
	vu.max_miter_scale = mesh->uniform.stroke_miter_radius_scale;
	vu.stroke_radius = mesh->uniform.stroke_radius;
	vu.dot_radius = mesh->uniform.dot_radius;
	vu.viewport_size = { (float)w, (float)h };
	vu.inlet_size = { (float)vw, (float)vh };
    vu.z_offset = z_offset;
    z_offset += 3e-6;
	resource_manager->write(&vu, 1, mesh->vert_uniform_handle, true);
	buffer& vert_uniform = resource_manager->buffer_for(mesh->vert_uniform_handle);
    c->VSSetConstantBuffers(0, 1, &vert_uniform.b);

    if (mesh->modded) {
        frag_uniform fu{};
        fu.gloss = mesh->uniform.gloss;
        fu.opacity = mesh->uniform.opacity;
        resource_manager->write(&fu, 1, mesh->frag_uniform_handle, true);
    }
    buffer& frag_uniform = resource_manager->buffer_for(mesh->frag_uniform_handle);
    c->PSSetConstantBuffers(0, 1, &frag_uniform.b);
}

void MCRenderer::render_single_mesh(struct tetramesh* mesh)
{
    if (mesh->uniform.opacity < FLT_EPSILON) {
        return;
    }

    /* uniforms and textures */
    if (!mesh->vert_uniform_handle) {
        if (!(mesh->vert_uniform_handle = resource_manager->register_id())) {
            return;
        }
    }
    if (!mesh->frag_uniform_handle) {
        if (!(mesh->frag_uniform_handle = resource_manager->register_id())) {
            return;
        }
    }

    UINT offset = 0;
    bool has_set_unis = false;

    /* tris */
    if (mesh->tri_count) {
        if (!mesh->tri_handle) {
            if (!(mesh->tri_handle = resource_manager->register_id())) {
                return;
            }
        }
        if (mesh->modded) {
            size_t size = 0;
            tri_in const* tris = tri_buffer_pointer_for(mesh, &size);
            resource_manager->write(tris, size * 3, mesh->tri_handle, false);
            free((tri_in *) tris);
        }

        /* render */
        buffer b = resource_manager->buffer_for(mesh->tri_handle);
        if (b.elems) {
            if (!has_set_unis) {
                has_set_unis = true;
                set_uniforms(mesh);
            }

            c->IASetInputLayout(tri_in_layout);
            c->IASetVertexBuffers(0, 1, &b.b, &b.size, &offset);
            c->VSSetShader(tri_vert_shader, nullptr, 0);
            c->PSSetShader(tri_frag_shader, nullptr, 0);

            c->PSSetSamplers(0, 1, &sampler);
            ID3D11ShaderResourceView* texture_view = resource_manager->get_texture_for_handle(mesh->texture_handle).first;

            c->PSSetShaderResources(0, 1, &texture_view);

            c->Draw(b.elems, 0);
        }
    }
    /* lins*/
    if (mesh->lin_count) {
        if (!mesh->lin_handle) {
            if (!(mesh->lin_handle = resource_manager->register_id())) {
                return;
            }
        }
        if (mesh->modded) {
            size_t size = 0;
            lin_in const* lins = lin_buffer_pointer_for(mesh, &size);
            resource_manager->write(lins, size, mesh->lin_handle, false);
            free((lin_in *) lins);
        }

        /* render */
        buffer b = resource_manager->buffer_for(mesh->lin_handle);
        if (b.elems) {
            if (!has_set_unis) {
                has_set_unis = true;
                set_uniforms(mesh);
            }

            c->IASetInputLayout(lin_in_layout);
            c->IASetVertexBuffers(0, 1, &b.b, &b.size, &offset);
            c->IASetVertexBuffers(1, 1, &lin_verts.b, &lin_verts.size, &offset);
            c->IASetIndexBuffer(lin_indices.b, DXGI_FORMAT_R16_UINT, 0);
            c->VSSetShader(lin_vert_shader, nullptr, 0);
            c->PSSetShader(lin_frag_shader, nullptr, 0);
            c->DrawIndexedInstanced(lin_indices.elems, b.elems, 0, 0, 0);
        }
    }
    /* dots */
    if (mesh->dot_count) {
        if (!mesh->dot_handle) {
            if (!(mesh->dot_handle = resource_manager->register_id())) {
                return;
            }
        }
        if (mesh->modded) {
            size_t size = 0;
            dot_in const* dots = dot_buffer_pointer_for(mesh, &size);
            resource_manager->write(dots, size, mesh->dot_handle, false);
            free((dot_in *) dots);
        }

        /* render */
        buffer b = resource_manager->buffer_for(mesh->dot_handle);
        if (b.elems) {
            if (!has_set_unis) {
                has_set_unis = true;
                set_uniforms(mesh);
            }

            c->IASetInputLayout(dot_in_layout);
            c->IASetVertexBuffers(0, 1, &b.b, &b.size, &offset);
            c->IASetVertexBuffers(1, 1, &dot_verts.b, &dot_verts.size, &offset);
            c->IASetIndexBuffer(dot_indices.b, DXGI_FORMAT_R16_UINT, 0);
            c->VSSetShader(dot_vert_shader, nullptr, 0);
            c->PSSetShader(dot_frag_shader, nullptr, 0);
            c->DrawIndexedInstanced(dot_indices.elems, b.elems, 0, 0, 0);
        }
    }
   

    if (mesh->modded) {
        mesh->modded = 0;
    }
}

void MCRenderer::render_mesh(void) { 
    this->z_offset = 0;
    viewport_read_lock(viewport);
    for (mc_ind_t i = 0; i < viewport->mesh_count; ++i) {
        render_single_mesh(viewport->meshes[i]);
    }
    viewport_read_unlock(viewport);
}

void MCRenderer::render(void) { 
    /* when exporting, only have exporter render */
    if (g_rm.top() != this->resource_manager) {
        return;
    }

#if RENDERDOC_DEBUG
    if (rdoc_api) rdoc_api->StartFrameCapture(NULL, NULL);
#endif

    /* clear render target and depth */
	float clearColor[4] = { bg.r, bg.g, bg.b, bg.a };
	c->ClearRenderTargetView(renderTargetView, clearColor);
    c->ClearDepthStencilView(depthStencilView, D3D11_CLEAR_DEPTH, 1.0f, 0);

	render_mesh();
	render_frame();

    c->Flush();

    D3D11_TEXTURE2D_DESC gpuDesc;
    renderBuffer->GetDesc(&gpuDesc);
    c->ResolveSubresource(resolveBuffer, 0, renderBuffer, 0, gpuDesc.Format);
    c->CopyResource(cpuBuffer, resolveBuffer);

#if RENDERDOC_DEBUG
    if (rdoc_api) rdoc_api->EndFrameCapture(NULL, NULL);
#endif
}

void MCRenderer::blit(char* dump) { 
	D3D11_MAPPED_SUBRESOURCE mappedResource;
    HRESULT hr = c->Map(cpuBuffer, 0, D3D11_MAP_READ, 0, &mappedResource);
    assert(SUCCEEDED(hr));

    for (int i = 0; i < h; ++i) {
		memcpy(dump + 4 * w * i, (char *) mappedResource.pData + i * mappedResource.RowPitch, 4 * w);
    }

    c->Unmap(cpuBuffer, 0);
}

MCRenderer::~MCRenderer() {
    delete resource_manager;

    if (renderBuffer) renderBuffer->Release();
    if (resolveBuffer) resolveBuffer->Release();
    if (cpuBuffer) cpuBuffer->Release();

    if (renderTargetView) renderTargetView->Release();
    if (depthStencilView) depthStencilView->Release();
    if (sampler) sampler->Release();

	/* shaders */
    if (frame_vert_shader) frame_vert_shader->Release();

    if (dot_vert_shader) dot_vert_shader->Release();
    if (lin_vert_shader) lin_vert_shader->Release();
    if (tri_vert_shader) tri_vert_shader->Release();

    if (tri_frag_shader) tri_frag_shader->Release();
    if (lin_frag_shader) lin_frag_shader->Release();
    if (dot_frag_shader) dot_frag_shader->Release();
    if (frame_frag_shader) frame_frag_shader->Release();

	/* layouts */
    if (frame_in_layout) frame_in_layout->Release();

    if (tri_in_layout) tri_in_layout->Release();
    if (lin_in_layout) lin_in_layout->Release();
    if (dot_in_layout) dot_in_layout->Release();
       
    /* buffers */
    if (frame_verts.b) frame_verts.b->Release();
    if (dot_verts.b) dot_verts.b->Release();
    if (dot_indices.b) dot_indices.b->Release();
    if (lin_verts.b) lin_verts.b->Release();
    if (lin_indices.b) lin_indices.b->Release();

    if (c) c->Release();
    if (d) d->Release();
}

extern "C" {
	mc_handle_t poll_texture(char const *path) {
        std::string str(path);
		return g_rm.top()->poll_texture(str);
	}

	void free_buffer(mc_handle_t handle) {
        g_rm.top()->deregister(handle);
	}
}
