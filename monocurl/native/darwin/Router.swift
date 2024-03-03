//
//  Router.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/6/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI

struct Router: View {
    @EnvironmentObject var environment: MonocurlEnvironment
    
    @State var url: URL? = nil
    
    var body: some View {
        Group {
            if url != nil {
                SceneEditorView($url, environment: environment)
            }
            else {
                LandingPage(url: $url)
            }
        }
    }
}
