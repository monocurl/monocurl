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

struct lin_in {
	float4 start_col : COL0;
	float4 end_col   : COL1;

	float3 start : POSITION0;
	float3 end   : POSITION1;

	float3 next_tan : TANGENT0;
	float3 tangent  : TANGENT1;
	float3 prev_tan : TANGENT2;

	int vertex_id: POSITION2;
};

struct v2f {
	float4 position: SV_POSITION;
	float4 col: COL;
};

v2f lin_vert(lin_in lin) {
	float t = (float) (lin.vertex_id >= 3);
	int sub = lin.vertex_id % 3;
	int extrude = sub != 0;

	float3 prev = lin.vertex_id == 1 ? lin.prev_tan : lin.tangent;
	float3 curr = lin.vertex_id == 5 ? lin.next_tan : lin.tangent;

	prev = mul(modelView, float4(prev, 0)).xyz;
	curr = mul(modelView, float4(curr, 0)).xyz;

	float4 model = mul(modelView, float4(lerp(lin.start, lin.end, t), 1));
	float3 proj_normal = cross(curr, float3(0, 0, 1));
	float3 used_normal = normalize(proj_normal);

	float3 prev_proj_normal = cross(prev, float3(0, 0, 1));
	float3 prev_used_normal = normalize(prev_proj_normal); 

	float scale = 2 * stroke_radius * model.z * proj._43 / inlet_size.x * extrude;

	float3 miter_clip = (prev_used_normal + used_normal) / 2;
	float projected = 1.0f / dot(miter_clip, used_normal);
	float3 un_clip = isinf(projected) ? float3(0, 0, 0) : miter_clip * projected;

	float miter_length_squared = dot(un_clip, un_clip);

	miter_clip = scale * miter_clip;
	un_clip = scale * un_clip;

	float compare_length = max_miter_scale * max_miter_scale;
	float3 full_normal = miter_length_squared > compare_length ? miter_clip : un_clip;
	model += float4(full_normal, 0);

	v2f ret;
	ret.position = mul(proj, model);
	ret.position.z -= z_offset + 1e-6;
	ret.col = lerp(lin.start_col, lin.end_col, t);

	return ret;
}

float4 lin_frag(v2f input) : SV_TARGET
{
	float4 res = input.col;
	res.w *= opacity;
    return res;
}
