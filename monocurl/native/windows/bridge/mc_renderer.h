#pragma once

extern "C" {
#include "monocurl.h"
#include "viewport.h"
}

#include <d3d11.h>
#include <d3dcompiler.h>
#include <DirectXMath.h>

struct float2 {
	float u, v;
};
struct float3 {
	float x, y, z;
};
struct float4 {
	float r, g, b, a;
};

/* c++/cli doesn't like <memory> ... */
struct buffer {
	UINT elems;
	UINT size;
	bool constant;

	ID3D11Buffer* b;

	buffer() : constant{ false }, elems {}, size{}, b{ nullptr } {}

	/* must be same size as initial */
	template<typename T>
	void write_fixed(ID3D11DeviceContext* c, int numel, T* src) {
		if (!numel) {
			this->size = sizeof(T);
			return;
		}

		D3D11_MAPPED_SUBRESOURCE mappedResource;
		c->Map(b, 0, D3D11_MAP_WRITE_DISCARD, 0, &mappedResource);
		memcpy(mappedResource.pData, src, numel * sizeof(T));
		this->size = sizeof(T);
		this->elems = elems;
		c->Unmap(b, 0);
	}

	template<typename T>
	static buffer vertex_buffer(ID3D11Device* d, int numel, T* source) {
		buffer ret;
		ret.elems = numel;
		ret.size = sizeof(T);
		ret.constant = 0;

		D3D11_BUFFER_DESC vertexBufferDesc = { 0 };
		vertexBufferDesc.ByteWidth = sizeof(T) * numel;
		vertexBufferDesc.BindFlags = D3D11_BIND_VERTEX_BUFFER;
		vertexBufferDesc.Usage = D3D11_USAGE_DYNAMIC;
		vertexBufferDesc.CPUAccessFlags = D3D11_CPU_ACCESS_WRITE;
		D3D11_SUBRESOURCE_DATA vertexSubresourceData = { source };

		HRESULT hResult = d->CreateBuffer(&vertexBufferDesc, &vertexSubresourceData, &ret.b);
		assert(SUCCEEDED(hResult));
		
		return ret;
	}

	template<typename T>
	static buffer index_buffer(ID3D11Device* d, int numel, T* source) {
		buffer ret;
		ret.elems = numel;
		ret.size = sizeof(T);
		ret.constant = 1;

		D3D11_BUFFER_DESC vertexBufferDesc = { 0 };
		vertexBufferDesc.ByteWidth = sizeof(T) * numel;
		vertexBufferDesc.BindFlags = D3D11_BIND_INDEX_BUFFER;
		vertexBufferDesc.Usage = D3D11_USAGE_DEFAULT;
		D3D11_SUBRESOURCE_DATA vertexSubresourceData = { source };

		HRESULT hResult = d->CreateBuffer(&vertexBufferDesc, &vertexSubresourceData, &ret.b);
		assert(SUCCEEDED(hResult));
		
		return ret;
	}

	template<typename T>
	static buffer cbuffer(ID3D11Device* d, T source) {
		buffer ret;
		ret.elems = 1;
		ret.size = sizeof(T);
		ret.constant = 1;

		D3D11_BUFFER_DESC vertexBufferDesc = { 0 };
		vertexBufferDesc.ByteWidth = sizeof(T);
		vertexBufferDesc.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
		vertexBufferDesc.Usage = D3D11_USAGE_DEFAULT;
		D3D11_SUBRESOURCE_DATA vertexSubresourceData = { &source };

		HRESULT hResult = d->CreateBuffer(&vertexBufferDesc, &vertexSubresourceData, &ret.b);
		assert(SUCCEEDED(hResult));
		
		return ret;
	}
};

class ResourceManager;

class MCRenderer {
private:
	bool in_presentation_mode{ false };
	bool matching_inlet{ false };

	int w, h;
	int vw, vh;

	ID3D11Device* d{ nullptr };
	ID3D11DeviceContext* c{ nullptr };
	ID3D11Debug* debug{ nullptr };

	ID3D11Texture2D* renderBuffer{ nullptr };
	ID3D11Texture2D* resolveBuffer{ nullptr };
	ID3D11Texture2D* cpuBuffer{ nullptr };

	ID3D11RenderTargetView* renderTargetView{ nullptr };
	ID3D11DepthStencilView* depthStencilView{ nullptr };
	ID3D11SamplerState* sampler{ nullptr };

	/* shaders */
	ID3D11VertexShader *frame_vert_shader{ nullptr };
	ID3D11PixelShader *frame_frag_shader{ nullptr };

	ID3D11VertexShader *dot_vert_shader{ nullptr };
	ID3D11VertexShader *lin_vert_shader{ nullptr };
	ID3D11VertexShader *tri_vert_shader{ nullptr };
	ID3D11PixelShader *dot_frag_shader{ nullptr };
	ID3D11PixelShader *lin_frag_shader{ nullptr };
	ID3D11PixelShader *tri_frag_shader{ nullptr };

	/* layouts */
	ID3D11InputLayout *frame_in_layout{ nullptr };
	ID3D11InputLayout *tri_in_layout{ nullptr };
	ID3D11InputLayout *lin_in_layout{ nullptr };
	ID3D11InputLayout *dot_in_layout{ nullptr };

	/* buffers */
	buffer frame_verts{};
	buffer dot_verts{};
	buffer dot_indices{};
	buffer lin_verts{};
	buffer lin_indices{};
	ResourceManager* resource_manager;

	/* cache */
	float4 bg;
	viewport::viewport_state state{ viewport::VIEWPORT_IDLE };
	viewport_camera camera;
	struct viewport* viewport;

	float near_ = 0, far_ = 0;
	float3 origin, up, forward;
	DirectX::XMMATRIX mv, p, norm;
	float z_offset;

	float aspect_ratio{ 1 };

	void init_device();
	void init_shader();
	
	void update_frame();
	void update_matrices();

	void set_uniforms(struct tetramesh* mesh);
	void render_single_mesh(struct tetramesh *mesh);
	void render_mesh();
	void render_frame();

public:
	MCRenderer();

	void set_screen_size(int width, int height, bool match_inlet);
	void set_presentation_mode(bool presenting);
	void recache(struct viewport* viewport);

	void render();

	void blit(char* dump);

	~MCRenderer();
};

extern "C" {
	unsigned int poll_texture(char const *path);
	void free_buffer(unsigned int handle);
}