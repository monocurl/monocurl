struct frame_in {
	float2 pos : POS;
	float4 color : COL;
};

struct frame_v2f {
	float4 position : SV_POSITION;
	float4 color : COL;
};

frame_v2f frame_vert(frame_in i) 
{
	frame_v2f output;
	output.position = float4(i.pos, 0.0f, 1.0f);
	output.color = i.color;

	return output;
}

float4 frame_frag(frame_v2f input) : SV_TARGET 
{
	return input.color;
}
