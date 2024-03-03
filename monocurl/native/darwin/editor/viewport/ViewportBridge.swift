//
//  ViewportBridge.swift
//  Monocurl
//
//  Created by Manu Bhat on 11/3/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation


struct ViewportCache {
    let ref: UnsafeMutablePointer<viewport>
    let lock: OpaquePointer!
    
    let aspectRatio: CGFloat
    let near: Float;
    let far: Float;
    
    let origin: simd_float3
    let forward: simd_float3
    let up: simd_float3
    
    let background: simd_float4
   
    let viewportState: viewport_state;
   
    // nonce makes sure that even in the case of duplicate projections, we still want a redraw (since meshes may have changed)
    let nonce: Int
    
    init(_ ref: UnsafeMutablePointer<viewport>, nonce: Int) {
        self.ref = ref;
        
        let raw = ref.pointee;

        self.lock = raw.lock;
        
        self.background = simd_float4(raw.background_color.x, raw.background_color.y, raw.background_color.z, raw.background_color.w)
        
        self.aspectRatio = CGFloat(raw.aspect_ratio);
        self.near = raw.camera.z_near;
        self.far = raw.camera.z_far;
        self.origin = simd_float3(raw.camera.origin.x, raw.camera.origin.y, raw.camera.origin.z);
        self.forward = simd_float3(raw.camera.forward.x, raw.camera.forward.y, raw.camera.forward.z);
        self.up = simd_float3(raw.camera.up.x, raw.camera.up.y, raw.camera.up.z);
        
        self.viewportState = raw.state;
        self.nonce = nonce
    }
}
