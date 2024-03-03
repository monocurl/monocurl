Texture2D _texture : register(t0);
SamplerState _sampler : register(s0);

cbuffer vertex_uniform : register(b0)
{
    float4x4 modelView;
    float4x4 proj;
    float4x4 norm;
	float2 viewport_size;
	float2 inlet_size;
	float stroke_radius;
	float max_miter_scale;
	float dot_radius;
	float z_offset;
};

cbuffer frag_uniform : register(b0)
{
	float opacity, gloss;
};

struct tri_in {
	float4 col : COL;
	float3 pos : POS;
	float3 norm : NORMAL;
	float2 uv : TEXCOORD;
};

struct v2f {
	float4 position: SV_POSITION;
	float3 model : POSITION;
	float3 normal : NORMAL;
	float4 col: COL;
	float2 uv : TEXCOORD;
};

v2f tri_vert(tri_in tri) {
	v2f ret;

	float4 model = mul(modelView, float4(tri.pos, 1));
	ret.model = model.xyz;
	ret.position = mul(proj, model);
	ret.position.z -= z_offset;
	ret.normal = mul(norm, float4(tri.norm, 0)).xyz;
	ret.col = tri.col;
	ret.uv = tri.uv;

	return ret;
}

float lighting(float3 norm, float3 model, float gloss) {
	float3 source = float3(1,1,0);
	float gamma = 3;
	float3 unit = normalize(source - model);
	float strength = dot(unit, norm);
	float ret = gloss * pow(strength, gamma);
	return isnan(ret) || isinf(ret) ? 0: ret;
}

float4 tri_frag(v2f input) : SV_TARGET
{
	float4 albedo = _texture.Sample(_sampler, input.uv);
	float4 res = albedo * input.col;
	res.w *= opacity;

	float specular = lighting(normalize(input.normal), input.model, gloss);

    return float4(res.xyz + (1 - res.xyz) * specular, res.w);
}

