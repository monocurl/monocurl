//
//  EditingScene.swift
//  Monocurl
//
//  Created by Manu Bhat on 10/9/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import SwiftUI

struct EditingScene: View {
    @EnvironmentObject private var environment: MonocurlEnvironment
    @ObservedObject var state: EditingSceneCache
    
    @State var showingConfirmation = false
    @State var deletionConfirmation: UnsafeMutablePointer<raw_slide_model>? = nil
    
    var body: some View {
        ScrollView(.vertical) {
            VStack(alignment: .trailing, spacing: 0) {
                ForEach(self.state.slides) { slide in
                    EditingSlide(state: slide, showingConfirmation: $showingConfirmation, deletionConfirmation: $deletionConfirmation)
                }
                Spacer()
                    .frame(minHeight: 400)
            }
        }
        .sheet(isPresented: $showingConfirmation) {
            VStack {
                Text("Confirm Deletion")
                Text("You cannot undo this action! ")
                    .font(.caption)
                    .padding(.bottom, 20)
                
                HStack {
                    Button("Cancel") {
                        deletionConfirmation = nil
                        showingConfirmation = false
                    }
                    
                    Button("Delete Slide") {
                        delete_slide(deletionConfirmation)
                        showingConfirmation = false
                        deletionConfirmation = nil
                    }
                }
            }
            .padding(40)
        }
        .onAppear {
            store = state
        }
    }
}
