# frozen_string_literal: true

module Inkoc
  module AST
    class Loop
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :body, :location

      def initialize(body, location)
        @body = body
        @location = location
      end

      def visitor_method
        :on_loop
      end
    end
  end
end
