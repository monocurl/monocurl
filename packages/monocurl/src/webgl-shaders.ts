export const TRIANGLE_VERTEX_SHADER = `#version 300 es
precision highp float;

layout(location = 0) in vec3 aPosition;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec4 aColor;
layout(location = 3) in vec2 aUv;

uniform vec3 uCameraPosition;
uniform vec3 uCameraRight;
uniform vec3 uCameraUp;
uniform vec3 uCameraForward;
uniform vec4 uCameraClip;
uniform vec2 uViewportScale;
uniform float uDepthBias;

out vec4 vColor;
out vec2 vUv;
out vec3 vModel;
out vec3 vNormal;

vec3 worldToCamera(vec3 world) {
  vec3 relative = world - uCameraPosition;
  return vec3(
    dot(relative, uCameraRight),
    dot(relative, uCameraUp),
    -dot(relative, uCameraForward)
  );
}

vec3 normalToCamera(vec3 normal) {
  return vec3(
    dot(normal, uCameraRight),
    dot(normal, uCameraUp),
    -dot(normal, uCameraForward)
  );
}

vec4 projectCamera(vec3 model) {
  float cameraX = model.x;
  float cameraY = model.y;
  float cameraZ = model.z;
  float tanHalfFov = max(uCameraClip.z, 0.05);
  float aspect = max(uCameraClip.w, 0.1);
  float nearClip = uCameraClip.x;
  float farClip = max(uCameraClip.y, nearClip + 0.0001);
  float clipW = -cameraZ;
  float clipX = cameraX / (tanHalfFov * aspect) * uViewportScale.x;
  float clipY = cameraY / tanHalfFov * uViewportScale.y;
  float clipZ = -farClip * cameraZ / (farClip - nearClip) - (farClip * nearClip) / (farClip - nearClip);
  clipZ -= uDepthBias * clipW;
  return vec4(clipX, clipY, clipZ * 2.0 - clipW, clipW);
}

void main() {
  vec3 model = worldToCamera(aPosition);
  gl_Position = projectCamera(model);
  vColor = aColor;
  vUv = aUv;
  vModel = model;
  vNormal = normalToCamera(aNormal);
}
`;

export const TRIANGLE_FRAGMENT_SHADER = `#version 300 es
precision highp float;

in vec4 vColor;
in vec2 vUv;
in vec3 vModel;
in vec3 vNormal;

uniform float uAlpha;
uniform float uGloss;

out vec4 outColor;

const vec3 LIGHT_SRC = vec3(1.0, 1.0, 0.0);
const float GAMMA = 3.0;
const float ALPHA_CUTOFF = 1.0 / 255.0;

void main() {
  vec3 normal = normalize(vNormal);
  vec3 lightDir = normalize(vModel - LIGHT_SRC);
  float specular = max(uGloss, 0.0) * pow(abs(dot(lightDir, normal)), GAMMA);
  vec3 litRgb = vColor.rgb + (vec3(1.0) - vColor.rgb) * specular;
  float alpha = vColor.a * uAlpha;
  if (alpha <= ALPHA_CUTOFF) {
    discard;
  }
  outColor = vec4(litRgb, alpha);
}
`;

export const LINE_VERTEX_SHADER = `#version 300 es
precision highp float;

layout(location = 0) in vec3 aPosition;
layout(location = 1) in vec4 aColor;
layout(location = 2) in vec3 aTangent;
layout(location = 3) in vec3 aPreviousTangent;
layout(location = 4) in float aExtrude;

uniform vec3 uCameraPosition;
uniform vec3 uCameraRight;
uniform vec3 uCameraUp;
uniform vec3 uCameraForward;
uniform vec4 uCameraClip;
uniform vec2 uViewportScale;
uniform vec4 uViewportAndLineWidth;
uniform vec2 uDepthBiasAndMiterScale;

out vec4 vColor;

vec3 worldToCamera(vec3 world) {
  vec3 relative = world - uCameraPosition;
  return vec3(
    dot(relative, uCameraRight),
    dot(relative, uCameraUp),
    -dot(relative, uCameraForward)
  );
}

vec3 vectorToCamera(vec3 vector) {
  return vec3(
    dot(vector, uCameraRight),
    dot(vector, uCameraUp),
    -dot(vector, uCameraForward)
  );
}

vec3 safeNormalize3(vec3 value) {
  if (dot(value, value) > 1e-6) {
    return normalize(value);
  }
  return vec3(0.0);
}

vec4 projectCamera(vec3 model) {
  float cameraX = model.x;
  float cameraY = model.y;
  float cameraZ = model.z;
  float tanHalfFov = max(uCameraClip.z, 0.05);
  float aspect = max(uCameraClip.w, 0.1);
  float nearClip = uCameraClip.x;
  float farClip = max(uCameraClip.y, nearClip + 0.0001);
  float clipW = -cameraZ;
  float clipX = cameraX / (tanHalfFov * aspect) * uViewportScale.x;
  float clipY = cameraY / tanHalfFov * uViewportScale.y;
  float clipZ = -farClip * cameraZ / (farClip - nearClip) - (farClip * nearClip) / (farClip - nearClip);
  clipZ -= uDepthBiasAndMiterScale.x * clipW;
  return vec4(clipX, clipY, clipZ * 2.0 - clipW, clipW);
}

void main() {
  vec3 model = worldToCamera(aPosition);
  vec2 viewport = max(uViewportAndLineWidth.xy, vec2(1.0));
  float radiusPx = max(uViewportAndLineWidth.z, 0.0);
  vec3 tangent = vectorToCamera(aTangent);
  vec3 previousTangent = vectorToCamera(aPreviousTangent);

  vec3 usedNormal = safeNormalize3(cross(tangent, vec3(0.0, 0.0, 1.0)));
  vec3 previousNormal = safeNormalize3(cross(previousTangent, vec3(0.0, 0.0, 1.0)));
  vec3 miterClip = 0.5 * (previousNormal + usedNormal);
  float miterDot = dot(miterClip, usedNormal);
  vec3 unclipped = vec3(0.0);
  if (abs(miterDot) > 1e-6) {
    unclipped = miterClip / miterDot;
  }

  float tanHalfFov = max(uCameraClip.z, 0.05);
  float aspect = max(uCameraClip.w, 0.1);
  float eyeDepth = max(-model.z, uCameraClip.x);
  float scale = 2.0 * radiusPx * eyeDepth * tanHalfFov * aspect / viewport.x * aExtrude;
  float maxMiterScale = max(uDepthBiasAndMiterScale.y, 0.0);
  vec3 fullNormal = unclipped * scale;
  if (dot(miterClip, miterClip) <= 1e-6 || dot(unclipped, unclipped) > maxMiterScale * maxMiterScale) {
    fullNormal = miterClip * scale;
  }

  gl_Position = projectCamera(model + fullNormal);
  vColor = vec4(aColor.rgb, aColor.a * uViewportAndLineWidth.w);
}
`;

export const SOLID_FRAGMENT_SHADER = `#version 300 es
precision highp float;

in vec4 vColor;
out vec4 outColor;

const float ALPHA_CUTOFF = 1.0 / 255.0;

void main() {
  if (vColor.a <= ALPHA_CUTOFF) {
    discard;
  }
  outColor = vColor;
}
`;

export const DOT_VERTEX_SHADER = `#version 300 es
precision highp float;

layout(location = 0) in vec3 aPosition;
layout(location = 1) in vec4 aColor;
layout(location = 2) in vec2 aLocal;

uniform vec3 uCameraPosition;
uniform vec3 uCameraRight;
uniform vec3 uCameraUp;
uniform vec3 uCameraForward;
uniform vec4 uCameraClip;
uniform vec2 uViewportScale;
uniform vec4 uViewportAndRadius;
uniform float uDepthBias;

out vec4 vColor;

struct ProjectedPoint {
  vec4 clip;
  vec2 ndc;
};

vec3 worldToCamera(vec3 world) {
  vec3 relative = world - uCameraPosition;
  return vec3(
    dot(relative, uCameraRight),
    dot(relative, uCameraUp),
    -dot(relative, uCameraForward)
  );
}

ProjectedPoint projectCamera(vec3 model) {
  float cameraX = model.x;
  float cameraY = model.y;
  float cameraZ = model.z;
  float tanHalfFov = max(uCameraClip.z, 0.05);
  float aspect = max(uCameraClip.w, 0.1);
  float nearClip = uCameraClip.x;
  float farClip = max(uCameraClip.y, nearClip + 0.0001);
  float clipW = -cameraZ;
  float clipX = cameraX / (tanHalfFov * aspect) * uViewportScale.x;
  float clipY = cameraY / tanHalfFov * uViewportScale.y;
  float clipZ = -farClip * cameraZ / (farClip - nearClip) - (farClip * nearClip) / (farClip - nearClip);
  clipZ -= uDepthBias * clipW;
  vec4 clip = vec4(clipX, clipY, clipZ, clipW);
  float invW = 1.0 / max(abs(clipW), 1e-6);
  return ProjectedPoint(clip, clip.xy * invW);
}

void main() {
  ProjectedPoint projected = projectCamera(worldToCamera(aPosition));
  vec2 viewport = max(uViewportAndRadius.xy, vec2(1.0));
  float radiusPx = max(uViewportAndRadius.z, 0.0);
  vec2 offsetNdc = aLocal * radiusPx * vec2(2.0 / viewport.x, 2.0 / viewport.y);
  vec2 positionXy = (projected.ndc + offsetNdc) * projected.clip.w;
  gl_Position = vec4(positionXy, projected.clip.z * 2.0 - projected.clip.w, projected.clip.w);
  vColor = vec4(aColor.rgb, aColor.a * uViewportAndRadius.w);
}
`;
