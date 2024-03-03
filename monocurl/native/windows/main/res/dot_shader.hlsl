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

struct dot_in {
	float4 col : COL;
	float3 pos : POSITION0;
	float3 norm : NORMAL;
	float theta : POSITION1;
};

struct v2f {
	float4 position: SV_POSITION;
	float4 col: COL;
};

v2f dot_vert(dot_in dot) {
	float3 helper = dot.norm + float3(1, abs(dot.norm.y) < 1e-3f, abs(dot.norm.z) < 1e-3f);
	float3 i_hat = normalize(cross(helper, dot.norm));
	float3 j_hat = normalize(cross(i_hat, dot.norm));

	float4 raw_model = mul(modelView, float4(dot.pos, 1));

	float scale = 2 * dot_radius * raw_model.z * proj._43 / viewport_size.x;
	float3 c = cos(dot.theta) * scale * i_hat;
	float3 s = sin(dot.theta) * scale * j_hat;

	float4 model = mul(modelView, float4(dot.pos + c + s, 1));
	float4 in_pos = mul(proj, model);

	v2f ret;
	ret.position = in_pos;
	ret.position.z -= z_offset + 2e-6;
	ret.col = dot.col;

	return ret;
}

float4 dot_frag(v2f input) : SV_TARGET
{
	float4 res = input.col;
	res.w *= opacity;
    return res;
}
